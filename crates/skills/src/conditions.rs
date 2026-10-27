//! `conditions` — classic diseases, monster contagion, poisons and cures
//! (M32, REQ-L99-065).
//!
//! **Source (Mode A, clean-room transcription of the measured extract):**
//! DFU `DiseaseEffect.cs:87-235` (lifecycle + `diseaseDataSources`),
//! `PoisonEffect.cs:25,41-46,137-285,306-330` (`Min/MaxMinutesToPoison`,
//! `Min/MaxRoundsOfPoison`, per-tick payload, drug positive-stat removal),
//! `FormulaHelper.cs:52,1259-1372` (contagion) and
//! `DaggerfallGuildServiceCureDisease.cs:47-95` (cure service cost).
//!
//! **Declared semantics (spec M32 §0):**
//! * `Random.Range(a, b)` is **max-exclusive** in C#; the tables below are
//!   transposed to **inclusive** bounds once and every draw goes through
//!   [`Rng64::range_inclusive`].
//! * Diseases tick on **whole classic days** (`minutes / 1440`); poisons tick
//!   on **classic minutes**. `daysPast == 0` is incubation: zero payload.
//! * `daysOfSymptomsMin == 0xFF` ([`PERMANENT_DISEASE`]) means the disease
//!   never expires by time — the payload still fires every day (source
//!   `UpdateDisease` → `IncrementDailyDiseaseEffects`, which is
//!   unconditional; golden vectors D2/D8/D11 pin this).
//! * Attribute damage accumulates; only the *live* read is clamped (spec §11
//!   risk 3). This module returns deltas, never clamped values.
//! * `Diseases` is `None = -1, WitchesPox = 0 … WizardFever = 16`.
//!
//! The live level, racial resistance and biography modifiers belong to M20 /
//! M23 (spec §10.2); this module consumes them as declared inputs.

use daggerfall_behavior::economy::{calculate_cost, holidays};

use crate::formula::calculate_trade_price;
use crate::rng::Rng64;

// ═══════════════════════════════════════════════════════════════════════════
// Shared layout
// ═══════════════════════════════════════════════════════════════════════════

/// Attribute columns of the `DiseaseData` matrix (source order).
pub const ATTRIBUTE_COUNT: usize = 8;
/// `Strength` column.
pub const STRENGTH: usize = 0;
/// `Intelligence` column.
pub const INTELLIGENCE: usize = 1;
/// `Willpower` column.
pub const WILLPOWER: usize = 2;
/// `Agility` column.
pub const AGILITY: usize = 3;
/// `Endurance` column.
pub const ENDURANCE: usize = 4;
/// `Personality` column.
pub const PERSONALITY: usize = 5;
/// `Speed` column.
pub const SPEED: usize = 6;
/// `Luck` column.
pub const LUCK: usize = 7;

/// Vital columns of the `DiseaseData` matrix (HEA, FAT, SPL).
pub const VITAL_COUNT: usize = 3;
/// `HEA` column — health.
pub const HEALTH: usize = 0;
/// `FAT` column — fatigue.
pub const FATIGUE: usize = 1;
/// `SPL` column — spell points.
pub const SPELL_POINTS: usize = 2;

/// Signed change set over the 8 attributes and the 3 vitals.
///
/// Negative = damage, positive = gain. The caller accumulates it into the
/// live entity; the clamp is applied when the live value is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VitalsDelta {
    /// Per-attribute change (STR, INT, WIL, AGI, END, PER, SPD, LUC).
    pub attributes: [i32; ATTRIBUTE_COUNT],
    /// Per-vital change (health, fatigue, spell points).
    pub vitals: [i32; VITAL_COUNT],
}

impl VitalsDelta {
    /// Adds another delta into this one.
    pub fn add(&mut self, other: &Self) {
        for (slot, value) in self.attributes.iter_mut().zip(other.attributes) {
            *slot += value;
        }
        for (slot, value) in self.vitals.iter_mut().zip(other.vitals) {
            *slot += value;
        }
    }

    /// `true` when every component is zero.
    pub fn is_zero(&self) -> bool {
        self.attributes.iter().all(|value| *value == 0)
            && self.vitals.iter().all(|value| *value == 0)
    }

    /// The inverse delta (used to withdraw positive drug bonuses).
    pub fn negated(&self) -> Self {
        let mut result = *self;
        for value in &mut result.attributes {
            *value = -*value;
        }
        for value in &mut result.vitals {
            *value = -*value;
        }
        result
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §1.1 · Classic diseases
// ═══════════════════════════════════════════════════════════════════════════

/// `daysOfSymptomsMin` value that marks a disease as permanent
/// (`DiseaseEffect.IsDiseasePermanent`).
pub const PERMANENT_DISEASE: u8 = 0xFF;
/// `daysOfSymptomsLeft` value of an ended disease (`EndDisease`).
pub const COMPLETED_DISEASE: u8 = 0xFE;

/// Number of concrete classic diseases (census §1.1 / `Diseases` enum).
pub const DISEASE_COUNT: usize = 17;

/// One row of `diseaseDataSources` (`DiseaseEffect.cs:217-235`).
///
/// Columns: `STR INT WIL AGI END PER SPD LUC HEA FAT SPL`, then the damage
/// range and the symptom-day range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiseaseData {
    /// Multiplier per attribute column (0 = not affected).
    pub stats: [u8; ATTRIBUTE_COUNT + VITAL_COUNT],
    /// Inclusive minimum daily damage roll.
    pub min_damage: u8,
    /// Inclusive maximum daily damage roll.
    pub max_damage: u8,
    /// Minimum `daysOfSymptomsLeft` (0xFF = permanent).
    pub days_min: u8,
    /// Maximum `daysOfSymptomsLeft` (unused when permanent).
    pub days_max: u8,
}

impl DiseaseData {
    /// `true` when the disease never expires by time (source
    /// `IsDiseasePermanent`).
    pub fn is_permanent(&self) -> bool {
        self.days_min == PERMANENT_DISEASE
    }
}

/// The 17 classic diseases (`Diseases` enum, `MagicAndEffectsEnums.cs:116-135`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Disease {
    /// Witches' Pox (index 0).
    WitchesPox = 0,
    /// Plague (index 1).
    Plague = 1,
    /// Yellow Fever (index 2).
    YellowFever = 2,
    /// Stomach Rot (index 3).
    StomachRot = 3,
    /// Consumption (index 4).
    Consumption = 4,
    /// Brain Fever (index 5).
    BrainFever = 5,
    /// Swamp Rot (index 6).
    SwampRot = 6,
    /// Caliron's Curse (index 7).
    CalironsCurse = 7,
    /// Cholera (index 8).
    Cholera = 8,
    /// Leprosy (index 9).
    Leprosy = 9,
    /// Wound Rot (index 10).
    WoundRot = 10,
    /// Red Death (index 11).
    RedDeath = 11,
    /// Blood Rot (index 12).
    BloodRot = 12,
    /// Typhoid Fever (index 13).
    TyphoidFever = 13,
    /// Dementia (index 14).
    Dementia = 14,
    /// Chrondiasis (index 15).
    Chrondiasis = 15,
    /// Wizard Fever (index 16).
    WizardFever = 16,
}

/// Transcribed `diseaseDataSources` (`DiseaseEffect.cs:217-235`), same order
/// as [`Disease`].
pub const DISEASE_DATA: [DiseaseData; DISEASE_COUNT] = [
    // STR INT WIL AGI END PER SPD LUC HEA FAT SPL · min max · daysMin daysMax
    DiseaseData { stats: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Witches' Pox
    DiseaseData { stats: [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1], min_damage: 3, max_damage: 30, days_min: 0xFF, days_max: 0xFF }, // Plague
    DiseaseData { stats: [0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0], min_damage: 5, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Yellow Fever
    DiseaseData { stats: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0], min_damage: 1, max_damage: 5, days_min: 0xFF, days_max: 0xFF }, // Stomach Rot
    DiseaseData { stats: [1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Consumption
    DiseaseData { stats: [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0], min_damage: 1, max_damage: 5, days_min: 0xFF, days_max: 0xFF }, // Brain Fever
    DiseaseData { stats: [1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Swamp Rot
    DiseaseData { stats: [1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0], min_damage: 5, max_damage: 10, days_min: 3, days_max: 18 }, // Caliron's Curse
    DiseaseData { stats: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], min_damage: 5, max_damage: 30, days_min: 0xFF, days_max: 0xFF }, // Cholera
    DiseaseData { stats: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0], min_damage: 5, max_damage: 30, days_min: 0xFF, days_max: 0xFF }, // Leprosy
    DiseaseData { stats: [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0], min_damage: 2, max_damage: 4, days_min: 0xFF, days_max: 0xFF }, // Wound Rot
    DiseaseData { stats: [0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Red Death
    DiseaseData { stats: [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0], min_damage: 5, max_damage: 10, days_min: 3, days_max: 18 }, // Blood Rot
    DiseaseData { stats: [0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Typhoid Fever
    DiseaseData { stats: [0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0], min_damage: 2, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Dementia
    DiseaseData { stats: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1], min_damage: 5, max_damage: 10, days_min: 0xFF, days_max: 0xFF }, // Chrondiasis
    DiseaseData { stats: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1], min_damage: 2, max_damage: 4, days_min: 3, days_max: 18 }, // Wizard Fever
];

impl Disease {
    /// Every classic disease in enum order.
    pub const ALL: [Disease; DISEASE_COUNT] = [
        Disease::WitchesPox,
        Disease::Plague,
        Disease::YellowFever,
        Disease::StomachRot,
        Disease::Consumption,
        Disease::BrainFever,
        Disease::SwampRot,
        Disease::CalironsCurse,
        Disease::Cholera,
        Disease::Leprosy,
        Disease::WoundRot,
        Disease::RedDeath,
        Disease::BloodRot,
        Disease::TyphoidFever,
        Disease::Dementia,
        Disease::Chrondiasis,
        Disease::WizardFever,
    ];

    /// Enum index (`(int)Diseases`).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Resolves an index in `0..DISEASE_COUNT`.
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Source identifier (`Diseases` enum member name).
    pub fn name(self) -> &'static str {
        match self {
            Disease::WitchesPox => "WitchesPox",
            Disease::Plague => "Plague",
            Disease::YellowFever => "YellowFever",
            Disease::StomachRot => "StomachRot",
            Disease::Consumption => "Consumption",
            Disease::BrainFever => "BrainFever",
            Disease::SwampRot => "SwampRot",
            Disease::CalironsCurse => "CalironsCurse",
            Disease::Cholera => "Cholera",
            Disease::Leprosy => "Leprosy",
            Disease::WoundRot => "WoundRot",
            Disease::RedDeath => "RedDeath",
            Disease::BloodRot => "BloodRot",
            Disease::TyphoidFever => "TyphoidFever",
            Disease::Dementia => "Dementia",
            Disease::Chrondiasis => "Chrondiasis",
            Disease::WizardFever => "WizardFever",
        }
    }

    /// Transcribed row of `diseaseDataSources`.
    pub fn data(self) -> &'static DiseaseData {
        &DISEASE_DATA[self.index()]
    }

    /// RSC message token of the contracted text (`100 + (int)disease`).
    pub fn message_token(self) -> u16 {
        100 + self.index() as u16
    }
}

/// One day-tick payload of a disease.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiseaseTick {
    /// Whole days advanced by this tick (`daysPast`).
    pub days: u32,
    /// Accumulated attribute/vital change (negative).
    pub delta: VitalsDelta,
    /// `true` when the disease ended on this tick.
    pub ended: bool,
    /// `daysOfSymptomsLeft` after the tick (0xFE = completed).
    pub days_of_symptoms_left: u8,
}

/// Lifecycle of one incumbent classic disease (`DiseaseEffect`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiseaseEffect {
    /// Contracted disease.
    pub disease: Disease,
    /// Classic day of the last applied tick (`lastDay`).
    pub last_day: u64,
    /// Day counter (`daysOfSymptomsLeft`); 0xFF = permanent, 0xFE = ended.
    pub days_of_symptoms_left: u8,
    /// `true` once `EndDisease()` ran (cure, expiry or ineligible host).
    pub completed: bool,
}

impl DiseaseEffect {
    /// `Start` (`DiseaseEffect.cs:87-110`): only a player with `Level >= 2`
    /// contracts the disease; otherwise it ends immediately. The duration is
    /// rolled only when the disease is not permanent.
    pub fn start(disease: Disease, day: u64, level: i32, rng: &mut Rng64) -> Self {
        if level < 2 {
            return Self {
                disease,
                last_day: day,
                days_of_symptoms_left: COMPLETED_DISEASE,
                completed: true,
            };
        }
        let data = *disease.data();
        let days_of_symptoms_left = if data.is_permanent() {
            PERMANENT_DISEASE
        } else {
            rng.range_inclusive(i32::from(data.days_min), i32::from(data.days_max)) as u8
        };
        Self {
            disease,
            last_day: day,
            days_of_symptoms_left,
            completed: false,
        }
    }

    /// `true` when the disease never expires by time.
    pub fn is_permanent(&self) -> bool {
        self.disease.data().is_permanent()
    }

    /// `CureDisease()` (`:255-258`) — the incumbent ends; already applied
    /// attribute damage is **not** rolled back (spec §11 risk 4).
    pub fn cure(&mut self) {
        self.days_of_symptoms_left = COMPLETED_DISEASE;
        self.completed = true;
    }

    /// Daily tick with an **injected** damage roll (`f` is called once per
    /// past day) — block A never consumes RNG.
    pub fn tick_with<F: FnMut() -> i32>(&mut self, current_day: u64, mut damage_roll: F) -> DiseaseTick {
        let mut tick = DiseaseTick {
            days: 0,
            delta: VitalsDelta::default(),
            ended: self.completed,
            days_of_symptoms_left: self.days_of_symptoms_left,
        };
        if self.completed || current_day <= self.last_day {
            return tick;
        }
        let days_past = u32::try_from(current_day - self.last_day).unwrap_or(u32::MAX);
        let data = *self.disease.data();
        tick.days = days_past;

        // Payload: one draw + one matrix application per past day. It runs
        // **before** the day counter is decremented, so a long time jump
        // applies every increment even when the disease "should" have ended
        // (golden vector D10).
        for _ in 0..days_past {
            let damage = damage_roll().max(0);
            for column in 0..ATTRIBUTE_COUNT {
                tick.delta.attributes[column] -= i32::from(data.stats[column]) * damage;
            }
            for column in 0..VITAL_COUNT {
                tick.delta.vitals[column] -= i32::from(data.stats[ATTRIBUTE_COUNT + column]) * damage;
            }
        }

        self.last_day = current_day;

        // Permanent diseases do not count down; the payload above still ran.
        if !data.is_permanent() {
            let left = i32::from(self.days_of_symptoms_left) - i32::try_from(days_past).unwrap_or(i32::MAX);
            if left <= 0 {
                self.days_of_symptoms_left = COMPLETED_DISEASE;
                self.completed = true;
            } else {
                self.days_of_symptoms_left = left as u8;
            }
        }
        tick.ended = self.completed;
        tick.days_of_symptoms_left = self.days_of_symptoms_left;
        tick
    }

    /// Daily tick with the source draw `Range(minDamage, maxDamage + 1)`.
    pub fn tick(&mut self, current_day: u64, rng: &mut Rng64) -> DiseaseTick {
        let data = *self.disease.data();
        let (min, max) = (i32::from(data.min_damage), i32::from(data.max_damage));
        self.tick_with(current_day, || rng.range_inclusive(min, max))
    }
}

/// The set of incumbent diseases of one host (`EntityEffectManager` book).
///
/// `AddState` is a no-op for a like-kind incumbent (`:106-110`), so the same
/// disease cannot be contracted twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiseaseBook {
    effects: Vec<DiseaseEffect>,
}

impl DiseaseBook {
    /// Empty book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Contracts `disease` if no incumbent of the same kind exists.
    ///
    /// Returns `true` when a new incumbent was added (`false` = duplicate or
    /// ineligible host, both of which leave the book untouched).
    pub fn infect(&mut self, disease: Disease, day: u64, level: i32, rng: &mut Rng64) -> bool {
        if self
            .effects
            .iter()
            .any(|effect| effect.disease == disease && !effect.completed)
        {
            return false;
        }
        if level < 2 {
            return false;
        }
        self.effects.push(DiseaseEffect::start(disease, day, level, rng));
        true
    }

    /// Ticks every incumbent and returns the aggregated payload.
    pub fn tick(&mut self, current_day: u64, rng: &mut Rng64) -> DiseaseTick {
        let mut total = DiseaseTick::default();
        for effect in &mut self.effects {
            let tick = effect.tick(current_day, rng);
            total.days = total.days.max(tick.days);
            total.delta.add(&tick.delta);
            total.ended |= tick.ended;
        }
        total.days_of_symptoms_left = self
            .effects
            .iter()
            .map(|effect| effect.days_of_symptoms_left)
            .min()
            .unwrap_or(0);
        total
    }

    /// Same aggregation with one **injected** damage roll per day (block A).
    pub fn tick_with<F: FnMut() -> i32>(&mut self, current_day: u64, mut damage: F) -> DiseaseTick {
        let mut total = DiseaseTick::default();
        for effect in &mut self.effects {
            let tick = effect.tick_with(current_day, &mut damage);
            total.days = total.days.max(tick.days);
            total.delta.add(&tick.delta);
            total.ended |= tick.ended;
        }
        total.days_of_symptoms_left = self
            .effects
            .iter()
            .map(|effect| effect.days_of_symptoms_left)
            .min()
            .unwrap_or(0);
        total
    }

    /// Number of active (non-completed) incumbents.
    pub fn count(&self) -> usize {
        self.effects.iter().filter(|effect| !effect.completed).count()
    }

    /// Active diseases, in contraction order.
    pub fn active(&self) -> Vec<Disease> {
        self.effects
            .iter()
            .filter(|effect| !effect.completed)
            .map(|effect| effect.disease)
            .collect()
    }

    /// `daysOfSymptomsLeft` of one incumbent, when present.
    pub fn days_left(&self, disease: Disease) -> Option<u8> {
        self.effects
            .iter()
            .find(|effect| effect.disease == disease)
            .map(|effect| effect.days_of_symptoms_left)
    }

    /// `true` when the disease still has a live incumbent.
    pub fn is_active(&self, disease: Disease) -> bool {
        self.effects
            .iter()
            .any(|effect| effect.disease == disease && !effect.completed)
    }

    /// `CureDisease()` on one disease.
    pub fn cure(&mut self, disease: Disease) -> bool {
        let mut cured = false;
        for effect in &mut self.effects {
            if effect.disease == disease && !effect.completed {
                effect.cure();
                cured = true;
            }
        }
        cured
    }

    /// `CureAllDiseases()` — the potion/service path. Returns the number of
    /// incumbents that ended.
    pub fn cure_all(&mut self) -> usize {
        let mut cured = 0;
        for effect in &mut self.effects {
            if !effect.completed {
                effect.cure();
                cured += 1;
            }
        }
        cured
    }

    /// Immunises the book against `disease` reappearing in the same instant
    /// (used by the demo to show the potion leaves no incumbent).
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §1.2 · Monster contagion
// ═══════════════════════════════════════════════════════════════════════════

/// `specialInfectionChance` (`FormulaHelper.cs:52`).
pub const SPECIAL_INFECTION_CHANCE: f64 = 0.6;

/// Source disease lists of `OnMonsterHit` (`FormulaHelper.cs:1272-1278`).
pub const DISEASE_LIST_A: [Disease; 1] = [Disease::Plague];
/// List B (rat, giant bat).
pub const DISEASE_LIST_B: [Disease; 3] =
    [Disease::Plague, Disease::StomachRot, Disease::BrainFever];
/// List C (zombie, mummy): the 11 entries of the source array.
pub const DISEASE_LIST_C: [Disease; 11] = [
    Disease::Plague,
    Disease::YellowFever,
    Disease::StomachRot,
    Disease::Consumption,
    Disease::BrainFever,
    Disease::SwampRot,
    Disease::Cholera,
    Disease::Leprosy,
    Disease::RedDeath,
    Disease::TyphoidFever,
    Disease::Dementia,
];

/// The four concrete infection classes next to the 17 classic diseases
/// (`Effects/Diseases/*.cs`): 17 + 4 = 21.
pub const INFECTION_CLASSES: [&str; 4] = [
    "LycanthropyInfection",
    "VampirismInfection",
    "WerewolfInfection",
    "WereboarInfection",
];

/// Number of concrete infection classes (census).
pub const INFECTION_COUNT: usize = INFECTION_CLASSES.len();

/// Hostile careers whose special attack is measured in §1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyKind {
    /// `MonsterCareers.Rat`.
    Rat,
    /// `MonsterCareers.GiantBat`.
    GiantBat,
    /// `MonsterCareers.Spider` (spell 66, no disease).
    Spider,
    /// `MonsterCareers.GiantScorpion` (spell 66, no disease).
    GiantScorpion,
    /// `MonsterCareers.Werewolf`.
    Werewolf,
    /// `MonsterCareers.Wereboar`.
    Wereboar,
    /// `MonsterCareers.Zombie`.
    Zombie,
    /// `MonsterCareers.Mummy`.
    Mummy,
    /// `MonsterCareers.Vampire`.
    Vampire,
    /// `MonsterCareers.VampireAncient`.
    VampireAncient,
    /// `MonsterCareers.Nymph` (fatigue damage, no disease).
    Nymph,
    /// `MonsterCareers.Lamia` (fatigue damage, no disease).
    Lamia,
}

/// Kind of lycanthropy carried by an infection class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LycanthropyKind {
    /// `WerewolfInfection`.
    Werewolf,
    /// `WereboarInfection`.
    Wereboar,
}

/// Outcome of one monster hit against the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contagion {
    /// Nothing transmitted (source `default` / spell / fatigue branches).
    None,
    /// A classic disease from list A, B or C.
    Disease(Disease),
    /// Stage-one vampirism (`VampirismInfection`).
    Vampirism,
    /// Stage-one lycanthropy (`WerewolfInfection` / `WereboarInfection`).
    Lycanthropy(LycanthropyKind),
}

/// Resistance gate declared as input (the live racial/career/biography
/// modifiers live in M20/M23, spec §10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResistanceGate {
    /// `Tolerance.Immune` (or a racial immunity) is active.
    pub immune: bool,
    /// Saving-throw percentage in `0..=100`; `0` = fully resisted.
    pub saving_throw: i32,
}

impl ResistanceGate {
    /// A host with no resistance at all.
    pub const NONE: ResistanceGate = ResistanceGate {
        immune: false,
        saving_throw: 100,
    };

    /// Whether the condition may land. `bypass_resistance` mirrors
    /// `InflictPoison(..., bypassResistance)` and the
    /// `AssignBundleFlags.BypassSavingThrows` used after a successful roll.
    pub fn allows(&self, bypass_resistance: bool) -> bool {
        bypass_resistance || (!self.immune && self.saving_throw > 0)
    }
}

/// `Dice100.SuccessRoll(chance)` (`Dice100.cs:14-24`): `roll(0..99) < chance`.
pub fn success_roll(chance: i32, roll: i32) -> bool {
    roll < chance
}

/// `OnMonsterHit` (`FormulaHelper.cs:1259-1372`).
///
/// * `roll` — the integer `Dice100` draw in `0..=99` (used by rat, bat,
///   zombie and mummy).
/// * `special_roll` — the float draw in `[0, 100)` used by the three
///   `specialInfectionChance` branches (`<= 0.6`) and the vampire fallback
///   (`<= 2.0`).
/// * `disease_pick` — index into the selected disease list (the source draws
///   `Random.Range(0, list.Length)`); it is reduced modulo the list length.
pub fn contact_infection(
    enemy: EnemyKind,
    roll: i32,
    special_roll: f64,
    disease_pick: usize,
) -> Contagion {
    fn pick(list: &[Disease], index: usize) -> Contagion {
        Contagion::Disease(list[index % list.len()])
    }
    match enemy {
        EnemyKind::Rat => {
            if success_roll(5, roll) {
                pick(&DISEASE_LIST_B, disease_pick)
            } else {
                Contagion::None
            }
        }
        EnemyKind::GiantBat => {
            if success_roll(2, roll) {
                pick(&DISEASE_LIST_B, disease_pick)
            } else {
                Contagion::None
            }
        }
        // Spell 66 (paralysis) is M23's effect model (spec §10.1).
        EnemyKind::Spider | EnemyKind::GiantScorpion => Contagion::None,
        EnemyKind::Werewolf => {
            if special_roll <= SPECIAL_INFECTION_CHANCE {
                Contagion::Lycanthropy(LycanthropyKind::Werewolf)
            } else {
                Contagion::None
            }
        }
        EnemyKind::Wereboar => {
            if special_roll <= SPECIAL_INFECTION_CHANCE {
                Contagion::Lycanthropy(LycanthropyKind::Wereboar)
            } else {
                Contagion::None
            }
        }
        EnemyKind::Zombie => {
            if success_roll(2, roll) {
                pick(&DISEASE_LIST_C, disease_pick)
            } else {
                Contagion::None
            }
        }
        EnemyKind::Mummy => {
            if success_roll(5, roll) {
                pick(&DISEASE_LIST_C, disease_pick)
            } else {
                Contagion::None
            }
        }
        EnemyKind::Vampire | EnemyKind::VampireAncient => {
            if special_roll <= SPECIAL_INFECTION_CHANCE {
                Contagion::Vampirism
            } else if special_roll <= 2.0 {
                pick(&DISEASE_LIST_A, disease_pick)
            } else {
                Contagion::None
            }
        }
        EnemyKind::Nymph | EnemyKind::Lamia => Contagion::None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §1.3 · Poisons and drugs
// ═══════════════════════════════════════════════════════════════════════════

/// First classic poison id (`Poisons.Nux_Vomica`).
pub const POISON_START_VALUE: i32 = 128;
/// Number of classic poisons/drugs (`128..=139`).
pub const POISON_COUNT: usize = 12;

/// Effect target of one per-minute poison roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoisonTarget {
    /// Health (`DecreaseHealth`).
    Health,
    /// Fatigue (`Decrease/IncreaseFatigue`).
    Fatigue,
    /// Spell points (`Decrease/IncreaseMagicka`).
    Magicka,
    /// Strength attribute mod.
    Strength,
    /// Intelligence attribute mod.
    Intelligence,
    /// Willpower attribute mod.
    Willpower,
    /// Agility attribute mod.
    Agility,
    /// Endurance attribute mod.
    Endurance,
    /// Personality attribute mod.
    Personality,
    /// Speed attribute mod.
    Speed,
    /// Luck attribute mod.
    Luck,
}

/// One per-minute roll of a poison (`IncrementPoisonEffects`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoisonDelta {
    /// Where the roll lands.
    pub target: PoisonTarget,
    /// Inclusive minimum.
    pub min: i32,
    /// Inclusive maximum.
    pub max: i32,
    /// `true` = the roll is a gain (drugs). Gains on attributes are withdrawn
    /// when the drug completes (`RemovePositiveStats`).
    pub positive: bool,
}

const fn delta(target: PoisonTarget, min: i32, max: i32, positive: bool) -> PoisonDelta {
    PoisonDelta {
        target,
        min,
        max,
        positive,
    }
}

/// One row of the poison tables (`PoisonEffect.cs:137-140,225-285`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoisonData {
    /// Inclusive minimum onset in minutes (`MinMinutesToPoison`).
    pub onset_min: u16,
    /// Inclusive maximum onset in minutes (`MaxMinutesToPoison`).
    pub onset_max: u16,
    /// Inclusive minimum duration in minutes (`MinRoundsOfPoison`).
    pub duration_min: u16,
    /// Inclusive maximum duration in minutes (`MaxRoundsOfPoison`).
    pub duration_max: u16,
    /// Per-minute payload.
    pub effects: &'static [PoisonDelta],
}

/// The 12 classic poisons/drugs (`Poisons` enum, `ItemEnums.cs:155-176`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Poison {
    /// `128 · Nux Vomica`.
    NuxVomica = 128,
    /// `129 · Arsenic`.
    Arsenic = 129,
    /// `130 · Moonseed`.
    Moonseed = 130,
    /// `131 · Drothweed`.
    Drothweed = 131,
    /// `132 · Somnalius`.
    Somnalius = 132,
    /// `133 · Pyrrhic Acid`.
    PyrrhicAcid = 133,
    /// `134 · Magebane`.
    Magebane = 134,
    /// `135 · Thyrwort`.
    Thyrwort = 135,
    /// `136 · Indulcet` (drug).
    Indulcet = 136,
    /// `137 · Sursum` (drug).
    Sursum = 137,
    /// `138 · Quaesto Vil` (drug).
    QuaestoVil = 138,
    /// `139 · Aegrotat` (drug).
    Aegrotat = 139,
}

/// Per-minute payloads (source order of `IncrementPoisonEffects`).
const NUX_EFFECTS: [PoisonDelta; 1] = [delta(PoisonTarget::Health, 2, 11, false)];
const ARSENIC_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Health, 2, 2, false),
    delta(PoisonTarget::Endurance, 1, 1, false),
];
const MOONSEED_EFFECTS: [PoisonDelta; 1] = [delta(PoisonTarget::Health, 1, 9, false)];
const DROTHWEED_EFFECTS: [PoisonDelta; 3] = [
    delta(PoisonTarget::Strength, 5, 9, false),
    delta(PoisonTarget::Agility, 1, 4, false),
    delta(PoisonTarget::Speed, 1, 4, false),
];
const SOMNALIUS_EFFECTS: [PoisonDelta; 1] = [delta(PoisonTarget::Fatigue, 10, 99, false)];
const PYRRHIC_EFFECTS: [PoisonDelta; 1] = [delta(PoisonTarget::Health, 1, 29, false)];
const MAGEBANE_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Willpower, 1, 4, false),
    delta(PoisonTarget::Magicka, 5, 14, false),
];
const THYRWORT_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Willpower, 5, 19, false),
    delta(PoisonTarget::Personality, 10, 19, false),
];
const INDULCET_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Fatigue, 10, 99, false),
    delta(PoisonTarget::Luck, 4, 9, true),
];
const SURSUM_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Intelligence, 10, 29, false),
    delta(PoisonTarget::Strength, 5, 19, true),
];
const QUAESTO_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Willpower, 1, 3, false),
    delta(PoisonTarget::Fatigue, 5, 9, true),
];
const AEGROTAT_EFFECTS: [PoisonDelta; 2] = [
    delta(PoisonTarget::Endurance, 1, 4, false),
    delta(PoisonTarget::Magicka, 5, 9, true),
];

/// Transcribed poison table, same order as [`Poison`].
pub const POISON_DATA: [PoisonData; POISON_COUNT] = [
    PoisonData { onset_min: 4, onset_max: 4, duration_min: 3, duration_max: 10, effects: &NUX_EFFECTS },
    PoisonData { onset_min: 10, onset_max: 10, duration_min: 20, duration_max: 1000, effects: &ARSENIC_EFFECTS },
    PoisonData { onset_min: 0, onset_max: 0, duration_min: 1, duration_max: 4, effects: &MOONSEED_EFFECTS },
    PoisonData { onset_min: 5, onset_max: 10, duration_min: 5, duration_max: 30, effects: &DROTHWEED_EFFECTS },
    PoisonData { onset_min: 0, onset_max: 0, duration_min: 2, duration_max: 10, effects: &SOMNALIUS_EFFECTS },
    PoisonData { onset_min: 0, onset_max: 0, duration_min: 1, duration_max: 2, effects: &PYRRHIC_EFFECTS },
    PoisonData { onset_min: 2, onset_max: 2, duration_min: 5, duration_max: 20, effects: &MAGEBANE_EFFECTS },
    PoisonData { onset_min: 0, onset_max: 0, duration_min: 1, duration_max: 3, effects: &THYRWORT_EFFECTS },
    PoisonData { onset_min: 2, onset_max: 12, duration_min: 2, duration_max: 6, effects: &INDULCET_EFFECTS },
    PoisonData { onset_min: 1, onset_max: 4, duration_min: 2, duration_max: 2, effects: &SURSUM_EFFECTS },
    PoisonData { onset_min: 2, onset_max: 12, duration_min: 1, duration_max: 4, effects: &QUAESTO_EFFECTS },
    PoisonData { onset_min: 0, onset_max: 0, duration_min: 5, duration_max: 20, effects: &AEGROTAT_EFFECTS },
];

impl Poison {
    /// Every classic poison/drug in enum order.
    pub const ALL: [Poison; POISON_COUNT] = [
        Poison::NuxVomica,
        Poison::Arsenic,
        Poison::Moonseed,
        Poison::Drothweed,
        Poison::Somnalius,
        Poison::PyrrhicAcid,
        Poison::Magebane,
        Poison::Thyrwort,
        Poison::Indulcet,
        Poison::Sursum,
        Poison::QuaestoVil,
        Poison::Aegrotat,
    ];

    /// `(int)Poison - startValue`.
    pub fn index(self) -> usize {
        self as usize - POISON_START_VALUE as usize
    }

    /// Resolves an id in `128..=139`.
    pub fn from_id(id: i32) -> Option<Self> {
        Self::ALL
            .get(usize::try_from(id - POISON_START_VALUE).ok()?)
            .copied()
    }

    /// Source identifier (`Poisons` enum member name).
    pub fn name(self) -> &'static str {
        match self {
            Poison::NuxVomica => "Nux_Vomica",
            Poison::Arsenic => "Arsenic",
            Poison::Moonseed => "Moonseed",
            Poison::Drothweed => "Drothweed",
            Poison::Somnalius => "Somnalius",
            Poison::PyrrhicAcid => "Pyrrhic_Acid",
            Poison::Magebane => "Magebane",
            Poison::Thyrwort => "Thyrwort",
            Poison::Indulcet => "Indulcet",
            Poison::Sursum => "Sursum",
            Poison::QuaestoVil => "Quaesto_Vil",
            Poison::Aegrotat => "Aegrotat",
        }
    }

    /// `true` for the four drugs (`IsDrugType`, indices 8..=11).
    pub fn is_drug(self) -> bool {
        self.index() >= 8
    }

    /// Transcribed row of the poison table.
    pub fn data(self) -> &'static PoisonData {
        &POISON_DATA[self.index()]
    }
}

/// Runtime state of one poison incumbent (`PoisonStates`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PoisonState {
    /// Countdown to onset.
    #[default]
    Waiting,
    /// Applying its payload once per minute.
    Active,
    /// Finished (payload applied for its whole duration).
    Complete,
}

/// Result of one game minute of a poison.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoisonStep {
    /// Payload applied this minute.
    pub applied: VitalsDelta,
    /// Positive attribute bonuses withdrawn (drug completion only).
    pub removed: VitalsDelta,
    /// State after the step.
    pub state: PoisonState,
}

impl VitalsDelta {
    /// Adds `amount` to the slot addressed by `target`.
    fn add_target(&mut self, target: PoisonTarget, amount: i32) {
        match target {
            PoisonTarget::Health => self.vitals[HEALTH] += amount,
            PoisonTarget::Fatigue => self.vitals[FATIGUE] += amount,
            PoisonTarget::Magicka => self.vitals[SPELL_POINTS] += amount,
            PoisonTarget::Strength => self.attributes[STRENGTH] += amount,
            PoisonTarget::Intelligence => self.attributes[INTELLIGENCE] += amount,
            PoisonTarget::Willpower => self.attributes[WILLPOWER] += amount,
            PoisonTarget::Agility => self.attributes[AGILITY] += amount,
            PoisonTarget::Endurance => self.attributes[ENDURANCE] += amount,
            PoisonTarget::Personality => self.attributes[PERSONALITY] += amount,
            PoisonTarget::Speed => self.attributes[SPEED] += amount,
            PoisonTarget::Luck => self.attributes[LUCK] += amount,
        }
    }
}

/// One incumbent poison (`PoisonEffect`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoisonEffect {
    /// Poison type.
    pub poison: Poison,
    /// Current lifecycle state.
    pub state: PoisonState,
    /// Minutes left before the first payload (`minutesToStart`).
    pub minutes_to_start: u16,
    /// Minutes of payload left (`minutesRemaining`).
    pub minutes_remaining: u16,
    /// Positive **attribute** mods accumulated by a drug.
    positive_attributes: [i32; ATTRIBUTE_COUNT],
    /// `RemovePositiveStats` already ran.
    positive_removed: bool,
}

impl PoisonEffect {
    /// `Start` (`PoisonEffect.cs:126-145`): rolls onset with
    /// `Range(min, max + 1)` and duration with `Range(min, max + 1)`.
    pub fn new(poison: Poison, rng: &mut Rng64) -> Self {
        let data = *poison.data();
        let onset = rng.range_inclusive(i32::from(data.onset_min), i32::from(data.onset_max));
        let duration =
            rng.range_inclusive(i32::from(data.duration_min), i32::from(data.duration_max));
        Self::with_rolls(poison, onset as u16, duration as u16)
    }

    /// Same lifecycle with **injected** onset/duration (block A is RNG-free).
    pub fn with_rolls(poison: Poison, onset: u16, duration: u16) -> Self {
        Self {
            poison,
            state: PoisonState::Waiting,
            minutes_to_start: onset,
            minutes_remaining: duration,
            positive_attributes: [0; ATTRIBUTE_COUNT],
            positive_removed: false,
        }
    }

    /// Advances exactly one game minute (`IncrementPoisonEffects`).
    ///
    /// `roll(min, max)` supplies one inclusive draw per effect slot; block A
    /// injects fixed values, block B passes [`Rng64::range_inclusive`].
    pub fn advance<F: FnMut(i32, i32) -> i32>(&mut self, mut roll: F) -> PoisonStep {
        let mut step = PoisonStep {
            applied: VitalsDelta::default(),
            removed: VitalsDelta::default(),
            state: self.state,
        };
        if self.state == PoisonState::Complete {
            return step;
        }

        // Countdown to onset: the minute that reaches zero starts the payload
        // in the same tick (source: `--minutesToStart` then fall through).
        if self.state == PoisonState::Waiting {
            self.minutes_to_start = self.minutes_to_start.saturating_sub(1);
            if self.minutes_to_start > 0 {
                return step;
            }
            self.state = PoisonState::Active;
        }

        for effect in self.poison.data().effects {
            let value = roll(effect.min, effect.max).clamp(effect.min, effect.max);
            if effect.positive {
                step.applied.add_target(effect.target, value);
                if let Some(slot) = attribute_slot(effect.target) {
                    self.positive_attributes[slot] += value;
                }
            } else {
                step.applied.add_target(effect.target, -value);
            }
        }

        self.minutes_remaining = self.minutes_remaining.saturating_sub(1);
        if self.minutes_remaining == 0 {
            self.state = PoisonState::Complete;
            // `CompletePoison`: a drug withdraws its positive attribute mods;
            // the damage already applied stays (golden vector P11).
            if self.poison.is_drug() && !self.positive_removed {
                let mut removed = VitalsDelta::default();
                for (slot, value) in self.positive_attributes.iter().enumerate() {
                    removed.attributes[slot] = -*value;
                }
                step.removed = removed;
                self.positive_removed = true;
            }
        }
        step.state = self.state;
        step
    }

    /// `CurePoison()` — ends the incumbent immediately. Positive drug mods
    /// are withdrawn; damage already applied persists.
    pub fn cure(&mut self) -> PoisonStep {
        let mut step = PoisonStep {
            applied: VitalsDelta::default(),
            removed: VitalsDelta::default(),
            state: PoisonState::Complete,
        };
        if self.state != PoisonState::Complete && self.poison.is_drug() && !self.positive_removed {
            for (slot, value) in self.positive_attributes.iter().enumerate() {
                step.removed.attributes[slot] = -*value;
            }
            self.positive_removed = true;
        }
        self.state = PoisonState::Complete;
        self.minutes_remaining = 0;
        step
    }

    /// Total worst-case health damage over `ticks` payload minutes.
    pub fn worst_health_damage(&self, ticks: u32) -> i32 {
        let per_tick: i32 = self
            .poison
            .data()
            .effects
            .iter()
            .filter(|effect| effect.target == PoisonTarget::Health && !effect.positive)
            .map(|effect| effect.max)
            .sum();
        per_tick * i32::try_from(ticks).unwrap_or(i32::MAX)
    }
}

/// Maps an attribute target to its [`VitalsDelta::attributes`] slot.
fn attribute_slot(target: PoisonTarget) -> Option<usize> {
    match target {
        PoisonTarget::Strength => Some(STRENGTH),
        PoisonTarget::Intelligence => Some(INTELLIGENCE),
        PoisonTarget::Willpower => Some(WILLPOWER),
        PoisonTarget::Agility => Some(AGILITY),
        PoisonTarget::Endurance => Some(ENDURANCE),
        PoisonTarget::Personality => Some(PERSONALITY),
        PoisonTarget::Speed => Some(SPEED),
        PoisonTarget::Luck => Some(LUCK),
        PoisonTarget::Health | PoisonTarget::Fatigue | PoisonTarget::Magicka => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §2 · Cure (temple service) and infection timing
// ═══════════════════════════════════════════════════════════════════════════

/// `250 * numberOfDiseases` (`DaggerfallGuildServiceCureDisease.cs:78`).
pub const CURE_BASE_COST: i64 = 250;
/// Days after which a vampiric/lycanthropic infection becomes full blown
/// (`VampirismInfection.ProgressDisease`: `daysPast > 3`).
pub const INFECTION_FULL_DAYS: u32 = 3;
/// Two classic weeks in minutes (the infection time jump).
pub const TWO_WEEKS_MINUTES: u64 = 20_160;
/// `DuskHour` of the classic clock.
pub const DUSK_HOUR: u64 = 18;

/// Input of the temple cure service (`CureDiseaseService`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CureServiceRequest {
    /// `EntityEffectManager.DiseaseCount`.
    pub diseases: i32,
    /// `TimeToBecomeVampireOrWerebeast != 0` (adds one unit).
    pub infection_pending: bool,
    /// Quality of the temple building.
    pub quality: u8,
    /// Arkay rank when the player is an Arkay member (rank discount).
    pub arkay_rank: Option<i32>,
    /// The visit happens on `North_Winds_Festival` (half price).
    pub north_winds_festival: bool,
    /// Player mercantile (bargaining stage).
    pub mercantile: i32,
    /// Player personality (bargaining stage).
    pub personality: i32,
}

impl CureServiceRequest {
    /// Number of billable units (`DiseaseCount + infection`).
    pub fn units(&self) -> i32 {
        self.diseases.max(0) + i32::from(self.infection_pending)
    }
}

/// `Guild.ReducedCureCost` for an Arkay temple
/// (`Temple.cs:418-422`): `(((10 - rank) << 8) / 10 * price) >> 8`.
pub fn arkay_reduced_cost(price: i64, rank: i32) -> i64 {
    let rank = rank.clamp(0, 10);
    let factor = (i64::from(10 - rank) << 8) / 10;
    (factor * price) >> 8
}

/// Full cure-service price (`DaggerfallGuildServiceCureDisease.cs:76-90`).
///
/// `0` means the service is free (no disease and no pending infection); the
/// free-holiday branch (`South_Winds_Prayer`, `First_Harvest`,
/// `Second_Harvest`) is exposed by [`cure_is_free`].
pub fn cure_service_cost(request: &CureServiceRequest) -> i64 {
    let units = request.units();
    if units <= 0 {
        return 0;
    }
    let mut base = CURE_BASE_COST * i64::from(units);
    if let Some(rank) = request.arkay_rank {
        base = arkay_reduced_cost(base, rank);
    }
    // `CalculateCost(baseCost, quality)` — M30's catalog stage (reused).
    let mut cost_before_bargaining = calculate_cost(base, request.quality, None);
    if request.north_winds_festival {
        cost_before_bargaining /= 2;
    }
    let quality = i32::from(request.quality).clamp(0, 20);
    let price = calculate_trade_price(
        i32::try_from(cost_before_bargaining).unwrap_or(i32::MAX),
        quality,
        false,
        request.mercantile,
        request.personality,
    );
    i64::from(price)
}

/// Holidays that cure every disease for free (`:56-65`).
pub fn cure_is_free(holiday: i32) -> bool {
    matches!(
        holiday,
        holidays::SOUTH_WINDS_PRAYER | holidays::FIRST_HARVEST | holidays::SECOND_HARVEST
    )
}

/// Infection stage of `VampirismInfection` / `LycanthropyInfection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfectionStage {
    /// Same day: nothing happens.
    Incubating,
    /// `daysPast > 0`: the warning dream.
    WarningDream,
    /// `daysPast > 3`: the fake death and full-blown stage two.
    FullBlown,
}

/// `ProgressDisease` stage of an infection (`VampirismInfection.cs:112-155`).
pub fn infection_stage(days_past: u32) -> InfectionStage {
    if days_past > INFECTION_FULL_DAYS {
        InfectionStage::FullBlown
    } else if days_past > 0 {
        InfectionStage::WarningDream
    } else {
        InfectionStage::Incubating
    }
}

/// Time jump applied when the infection becomes full blown
/// (`VampirismInfection.cs:184-186`):
/// `2 weeks + (DuskHour + 1 - hour) hours`, which lands at 19:00.
pub fn infection_time_jump_minutes(current_hour: u64) -> u64 {
    let dusk_delta = (DUSK_HOUR + 1).saturating_sub(current_hour % 24) * 60;
    TWO_WEEKS_MINUTES + dusk_delta
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disease_table_matches_source_rows() {
        assert_eq!(DISEASE_DATA.len(), DISEASE_COUNT);
        // Plague: every attribute except INT and every vital.
        assert_eq!(
            Disease::Plague.data().stats,
            [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        );
        assert_eq!((Disease::Plague.data().min_damage, Disease::Plague.data().max_damage), (3, 30));
        assert!(Disease::Plague.data().is_permanent());
        assert!(!Disease::CalironsCurse.data().is_permanent());
        assert_eq!((Disease::CalironsCurse.data().days_min, Disease::CalironsCurse.data().days_max), (3, 18));
        assert_eq!(Disease::WizardFever.message_token(), 116);
    }

    #[test]
    fn disease_tick_accumulates_every_past_day_before_ending() {
        let mut rng = Rng64::new(0x5243_AF63);
        let mut effect = DiseaseEffect::start(Disease::CalironsCurse, 10, 5, &mut rng);
        effect.days_of_symptoms_left = 3;
        let tick = effect.tick_with(15, || 5);
        assert_eq!(tick.days, 5);
        // Five days of damage even though only three symptom days were left.
        assert_eq!(tick.delta.attributes[STRENGTH], -25);
        assert_eq!(tick.delta.attributes[AGILITY], -25);
        assert_eq!(tick.delta.attributes[SPEED], -25);
        assert!(tick.ended);
        assert_eq!(tick.days_of_symptoms_left, COMPLETED_DISEASE);
    }

    #[test]
    fn disease_incubation_day_is_silent() {
        let mut rng = Rng64::new(1);
        let mut effect = DiseaseEffect::start(Disease::Plague, 40, 5, &mut rng);
        let tick = effect.tick(40, &mut rng);
        assert_eq!(tick.days, 0);
        assert!(tick.delta.is_zero());
        assert!(!tick.ended);
    }

    #[test]
    fn permanent_disease_never_expires() {
        let mut rng = Rng64::new(7);
        let mut effect = DiseaseEffect::start(Disease::Plague, 0, 5, &mut rng);
        let tick = effect.tick(10_000, &mut rng);
        assert!(!tick.ended);
        assert_eq!(tick.days_of_symptoms_left, PERMANENT_DISEASE);
        assert_eq!(effect.last_day, 10_000);
    }

    #[test]
    fn level_one_host_cannot_contract() {
        let mut rng = Rng64::new(3);
        let effect = DiseaseEffect::start(Disease::Plague, 0, 1, &mut rng);
        assert!(effect.completed);
        let mut book = DiseaseBook::new();
        assert!(!book.infect(Disease::Plague, 0, 1, &mut rng));
        assert!(book.is_empty());
    }

    #[test]
    fn duplicate_disease_is_not_added_twice() {
        let mut rng = Rng64::new(9);
        let mut book = DiseaseBook::new();
        assert!(book.infect(Disease::Plague, 0, 5, &mut rng));
        assert!(!book.infect(Disease::Plague, 0, 5, &mut rng));
        assert_eq!(book.count(), 1);
    }

    #[test]
    fn contagion_uses_success_roll_and_special_chance() {
        assert_eq!(
            contact_infection(EnemyKind::Rat, 4, 50.0, 0),
            Contagion::Disease(Disease::Plague)
        );
        assert_eq!(contact_infection(EnemyKind::Rat, 5, 50.0, 0), Contagion::None);
        assert_eq!(
            contact_infection(EnemyKind::Vampire, 99, 0.6, 0),
            Contagion::Vampirism
        );
        assert_eq!(
            contact_infection(EnemyKind::Vampire, 99, 1.5, 0),
            Contagion::Disease(Disease::Plague)
        );
        assert_eq!(contact_infection(EnemyKind::Vampire, 99, 2.5, 0), Contagion::None);
        assert_eq!(
            contact_infection(EnemyKind::Werewolf, 99, 0.1, 0),
            Contagion::Lycanthropy(LycanthropyKind::Werewolf)
        );
        assert_eq!(contact_infection(EnemyKind::Spider, 0, 0.0, 0), Contagion::None);
        assert_eq!(DISEASE_LIST_C.len(), 11);
        assert_eq!(INFECTION_COUNT, 4);
    }

    #[test]
    fn poison_tables_match_source_ranges() {
        assert_eq!(POISON_DATA.len(), POISON_COUNT);
        let nux = Poison::NuxVomica.data();
        assert_eq!((nux.onset_min, nux.onset_max), (4, 4));
        assert_eq!((nux.duration_min, nux.duration_max), (3, 10));
        assert_eq!(nux.effects[0].min, 2);
        assert_eq!(nux.effects[0].max, 11);
        assert_eq!(Poison::Sursum.data().duration_min, 2);
        assert_eq!(Poison::Sursum.data().duration_max, 2);
        assert_eq!(Poison::Arsenic.data().duration_max, 1000);
        assert_eq!(Poison::PyrrhicAcid.data().duration_min, 1);
        assert_eq!(Poison::PyrrhicAcid.data().duration_max, 2);
        assert!(Poison::Indulcet.is_drug());
        assert!(!Poison::NuxVomica.is_drug());
    }

    #[test]
    fn poison_waits_for_onset_then_ticks_each_minute() {
        let mut poison = PoisonEffect::with_rolls(Poison::NuxVomica, 4, 3);
        let mut applied = VitalsDelta::default();
        for minute in 0..7 {
            let step = poison.advance(|_, max| max);
            applied.add(&step.applied);
            if minute < 3 {
                assert_eq!(step.state, PoisonState::Waiting, "onsets at minute 4");
            }
            if minute == 3 {
                assert_eq!(step.state, PoisonState::Active);
                assert_eq!(step.applied.vitals[HEALTH], -11);
            }
        }
        assert_eq!(poison.state, PoisonState::Complete);
        assert_eq!(applied.vitals[HEALTH], -33);
    }

    #[test]
    fn drug_completion_withdraws_positive_attribute_mods_only() {
        let mut poison = PoisonEffect::with_rolls(Poison::Sursum, 0, 2);
        let mut net = VitalsDelta::default();
        let mut removed = VitalsDelta::default();
        for _ in 0..2 {
            let step = poison.advance(|_, max| max);
            net.add(&step.applied);
            removed.add(&step.removed);
        }
        assert_eq!(poison.state, PoisonState::Complete);
        // Two ticks: INT -29 each and STR +19 each.
        assert_eq!(net.attributes[INTELLIGENCE], -58);
        assert_eq!(net.attributes[STRENGTH], 38);
        // The gain is withdrawn; the damage persists.
        assert_eq!(removed.attributes[STRENGTH], -38);
        assert_eq!(removed.attributes[INTELLIGENCE], 0);
    }

    #[test]
    fn poison_worst_health_damage_uses_inclusive_bound() {
        assert_eq!(Poison::NuxVomica.data().duration_max, 10);
        let poison = PoisonEffect::with_rolls(Poison::NuxVomica, 4, 10);
        assert_eq!(poison.worst_health_damage(10), 110);
        let arsenic = PoisonEffect::with_rolls(Poison::Arsenic, 10, 1000);
        assert_eq!(arsenic.worst_health_damage(1000), 2000);
    }

    #[test]
    fn cure_service_golden_prices() {
        let request = |diseases: i32, north_winds: bool| CureServiceRequest {
            diseases,
            infection_pending: false,
            quality: 10,
            arkay_rank: None,
            north_winds_festival: north_winds,
            mercantile: 50,
            personality: 50,
        };
        assert_eq!(cure_service_cost(&request(2, false)), 562);
        assert_eq!(cure_service_cost(&request(1, false)), 281);
        assert_eq!(cure_service_cost(&request(1, true)), 140);
        assert_eq!(cure_service_cost(&request(0, false)), 0);
        // Arkay rank 10 ⇒ base 0; `CalculateCost` floors at 1 unit ⇒ 1 gold.
        let mut arkay = request(1, false);
        arkay.arkay_rank = Some(10);
        assert_eq!(cure_service_cost(&arkay), 1);
        // Rank 5 halves the base first: 250 → 125 → 250 → 140.
        let mut half = request(1, false);
        half.arkay_rank = Some(5);
        assert_eq!(cure_service_cost(&half), 140);
        assert!(cure_is_free(holidays::FIRST_HARVEST));
        assert!(!cure_is_free(holidays::NORTH_WINDS_FESTIVAL));
    }

    #[test]
    fn infection_stages_and_time_jump() {
        assert_eq!(infection_stage(0), InfectionStage::Incubating);
        assert_eq!(infection_stage(1), InfectionStage::WarningDream);
        assert_eq!(infection_stage(3), InfectionStage::WarningDream);
        assert_eq!(infection_stage(4), InfectionStage::FullBlown);
        // 08:00 + jump lands at 19:00 two weeks later.
        let jump = infection_time_jump_minutes(8);
        assert_eq!(jump, TWO_WEEKS_MINUTES + 11 * 60);
        assert_eq!(infection_time_jump_minutes(19), TWO_WEEKS_MINUTES);
    }
}
