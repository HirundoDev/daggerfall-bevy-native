//! Diseases family — 20 registered templates from DFU `0038a1e6`
//! (`Assets/Scripts/Game/MagicAndEffects/Effects/Diseases/*.cs`).
//!
//! Census: **22** `.cs` files, **2** abstract bases that are *not* templates —
//! `DiseaseEffect.cs:21` (`public abstract class DiseaseEffect : IncumbentEffect`)
//! and `LycanthropyInfection.cs:22` (`public abstract class LycanthropyInfection :
//! DiseaseEffect`) — ⇒ **20** concrete templates, one row each, in DFU file order.
//! DFU does not register the abstract bases either (`EntityEffectBroker.cs:1126-1134`
//! filters `IsAbstract`, contract §1); the base payload runs from its subclasses and
//! each subclass row cites it.
//!
//! Two sources of the 15 bytes:
//! * the **17 classic** diseases do not declare `new DiseaseData(...)` inline. Their
//!   `SetProperties()` calls `diseaseData = GetClassicDiseaseData(diseaseType)`
//!   (`DiseaseEffect.cs:210-239`), which indexes the shared table at
//!   `DiseaseEffect.cs:217-236` by `(int)diseaseType` — the `Diseases` enum
//!   (`MagicAndEffectsEnums.cs:116-136`, `None = -1` … `WizardFever = 16`). Each row
//!   below copies its 15-byte table row literally and cites the table line;
//! * `VampirismInfection.cs:49` and the two lycanthropy infections (inheriting
//!   `LycanthropyInfection.cs:33`) declare their own all-zero permanent matrix.
//!
//! `DiseaseData` is one struct literal of exactly 15 `u8` fields, in the C# constructor
//! order (`MagicAndEffectsStructs.cs:232-256`):
//! `STR, INT, WIL, AGI, END, PER, SPD, LUC, HEA, FAT, SPL, minDamage, maxDamage,
//! daysOfSymptomsMin, daysOfSymptomsMax`. `daysOfSymptomsMin == 0xFF` ⇒ permanent
//! (`DiseaseEffect.cs:241-244`).
//!
//! What a disease template declares, and what it does not:
//! * `properties.Key = GetClassicDiseaseEffectKey(diseaseType)` =
//!   `string.Format("Disease-{0}", diseaseType.ToString())` (`DiseaseEffect.cs:260-263`),
//!   or the class literal (`Vampirism-Infection`, `Wereboar-Infection`,
//!   `Werewolf-Infection`);
//! * `properties.ShowSpellIcon = false` (not modeled by `EffectProperties`);
//! * `classicDiseaseType` and `diseaseData` (fields of the effect, not of the
//!   properties; `diseaseData` becomes `Payload::Disease`);
//! * **no** `properties.ClassicKey` ⇒ `classic_key` is `0` (contract §3);
//! * **no** `properties.MagicSkill` ⇒ `MagicSkill::None` (`EntityEffect.cs:302`
//!   default);
//! * **no** `SupportDuration` / `SupportChance` / `SupportMagnitude`, no cost block
//!   and no `variantCount` ⇒ plain `EffectProperties::new` with defaults.
//!
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md`
//! (§2 vocabulary, §3 builder, §4.1 Diseases mapping).
//!
//! Each row cites its source `file:line` on the `///` comment immediately above it.
//! A doc comment on an expression (array element) is ignored by rustdoc rather than
//! shown, so `unused_doc_comments` is allowed module-wide to keep the build
//! warning-free.
#![allow(unused_doc_comments)]

use crate::magic::{DiseaseData, EffectProperties, MagicSkill, Payload};

// ── the 17 classic `DiseaseData` rows (`DiseaseEffect.cs:217-236`) ──────────

/// `DiseaseEffect.cs:219` — row 0, `Diseases.WitchesPox` (`MagicAndEffectsEnums.cs:119`);
/// comment in source: `Witches' Pox`.
const WITCHES_POX_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 1,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:220` — row 1, `Diseases.Plague` (`MagicAndEffectsEnums.cs:120`).
const PLAGUE_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 1,
    agility: 1,
    endurance: 1,
    personality: 1,
    speed: 1,
    luck: 1,
    health: 1,
    fatigue: 1,
    spell_points: 1,
    min_damage: 3,
    max_damage: 30,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:221` — row 2, `Diseases.YellowFever` (`MagicAndEffectsEnums.cs:121`).
const YELLOW_FEVER_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 1,
    agility: 0,
    endurance: 1,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 5,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:222` — row 3, `Diseases.StomachRot` (`MagicAndEffectsEnums.cs:122`).
const STOMACH_ROT_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 1,
    max_damage: 5,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:223` — row 4, `Diseases.Consumption` (`MagicAndEffectsEnums.cs:123`).
const CONSUMPTION_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 1,
    agility: 1,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:224` — row 5, `Diseases.BrainFever` (`MagicAndEffectsEnums.cs:124`).
const BRAIN_FEVER_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 1,
    agility: 0,
    endurance: 0,
    personality: 1,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 1,
    max_damage: 5,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:225` — row 6, `Diseases.SwampRot` (`MagicAndEffectsEnums.cs:125`).
const SWAMP_ROT_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 1,
    agility: 1,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:226` — row 7, `Diseases.CalironsCurse`
/// (`MagicAndEffectsEnums.cs:126`); finite 3–18 days, not permanent.
const CALIRONS_CURSE_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 0,
    agility: 1,
    endurance: 0,
    personality: 0,
    speed: 1,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 5,
    max_damage: 10,
    days_min: 3,
    days_max: 18,
};

/// `DiseaseEffect.cs:227` — row 8, `Diseases.Cholera` (`MagicAndEffectsEnums.cs:127`).
const CHOLERA_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 1,
    willpower: 1,
    agility: 1,
    endurance: 1,
    personality: 1,
    speed: 1,
    luck: 1,
    health: 1,
    fatigue: 1,
    spell_points: 1,
    min_damage: 5,
    max_damage: 30,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:228` — row 9, `Diseases.Leprosy` (`MagicAndEffectsEnums.cs:128`).
const LEPROSY_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 1,
    willpower: 1,
    agility: 1,
    endurance: 1,
    personality: 1,
    speed: 1,
    luck: 1,
    health: 1,
    fatigue: 1,
    spell_points: 0,
    min_damage: 5,
    max_damage: 30,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:229` — row 10, `Diseases.WoundRot` (`MagicAndEffectsEnums.cs:129`).
const WOUND_ROT_DATA: DiseaseData = DiseaseData {
    strength: 1,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 1,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 4,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:230` — row 11, `Diseases.RedDeath` (`MagicAndEffectsEnums.cs:130`).
const RED_DEATH_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 1,
    personality: 1,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 1,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:231` — row 12, `Diseases.BloodRot` (`MagicAndEffectsEnums.cs:131`);
/// finite 3–18 days, not permanent.
const BLOOD_ROT_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 1,
    agility: 0,
    endurance: 0,
    personality: 1,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 5,
    max_damage: 10,
    days_min: 3,
    days_max: 18,
};

/// `DiseaseEffect.cs:232` — row 13, `Diseases.TyphoidFever`
/// (`MagicAndEffectsEnums.cs:132`).
const TYPHOID_FEVER_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 1,
    willpower: 0,
    agility: 0,
    endurance: 1,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 1,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:233` — row 14, `Diseases.Dementia` (`MagicAndEffectsEnums.cs:133`).
const DEMENTIA_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 1,
    willpower: 1,
    agility: 0,
    endurance: 0,
    personality: 1,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 2,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:234` — row 15, `Diseases.Chrondiasis`
/// (`MagicAndEffectsEnums.cs:134`).
const CHRONDIASIS_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 1,
    willpower: 0,
    agility: 0,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 1,
    min_damage: 5,
    max_damage: 10,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `DiseaseEffect.cs:235` — row 16, `Diseases.WizardFever`
/// (`MagicAndEffectsEnums.cs:135`); finite 3–18 days, not permanent.
const WIZARD_FEVER_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 1,
    willpower: 0,
    agility: 0,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 1,
    min_damage: 2,
    max_damage: 4,
    days_min: 3,
    days_max: 18,
};

// ── the 3 non-classic matrices declared inline ─────────────────────────────

/// `VampirismInfection.cs:49` — `new DiseaseData(0, …, 0xFF, 0xFF)`: permanent
/// no-effect disease, custom lifecycle (`:35` key constant, `:48`
/// `classicDiseaseType = Diseases.None`).
const VAMPIRISM_INFECTION_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 0,
    max_damage: 0,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// `LycanthropyInfection.cs:33` — shared by `WereboarInfection` and
/// `WerewolfInfection` through `base.SetProperties()` (`WereboarInfection.cs:27`,
/// `WerewolfInfection.cs:27`): permanent no-effect disease, custom lifecycle.
const LYCANTHROPY_INFECTION_DATA: DiseaseData = DiseaseData {
    strength: 0,
    intelligence: 0,
    willpower: 0,
    agility: 0,
    endurance: 0,
    personality: 0,
    speed: 0,
    luck: 0,
    health: 0,
    fatigue: 0,
    spell_points: 0,
    min_damage: 0,
    max_damage: 0,
    days_min: 0xFF,
    days_max: 0xFF,
};

/// The 20 Diseases templates of DFU `0038a1e6`, in file order.
pub const DISEASES: &[EffectProperties] = &[
    /// `BloodRot.cs:16` (`class BloodRot : DiseaseEffect`), `:18` (`diseaseType =
    /// Diseases.BloodRot`), `:22` (`properties.Key = GetClassicDiseaseEffectKey
    /// (diseaseType)` → `Disease-BloodRot`, `DiseaseEffect.cs:260-263`), `:25`
    /// (`diseaseData = GetClassicDiseaseData(diseaseType)`); bytes at
    /// `DiseaseEffect.cs:231` (row 12). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-BloodRot",
        0,
        MagicSkill::None,
        &[Payload::Disease(&BLOOD_ROT_DATA)],
    ),
    /// `BrainFever.cs:16`, `:18` (`Diseases.BrainFever`), `:22` (`Key` →
    /// `Disease-BrainFever`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:224` (row 5). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-BrainFever",
        0,
        MagicSkill::None,
        &[Payload::Disease(&BRAIN_FEVER_DATA)],
    ),
    /// `CalironsCurse.cs:16`, `:18` (`Diseases.CalironsCurse`), `:22` (`Key` →
    /// `Disease-CalironsCurse`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:226` (row 7). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-CalironsCurse",
        0,
        MagicSkill::None,
        &[Payload::Disease(&CALIRONS_CURSE_DATA)],
    ),
    /// `Cholera.cs:16`, `:18` (`Diseases.Cholera`), `:22` (`Key` → `Disease-Cholera`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:227` (row 8).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Cholera",
        0,
        MagicSkill::None,
        &[Payload::Disease(&CHOLERA_DATA)],
    ),
    /// `Chrondiasis.cs:16`, `:18` (`Diseases.Chrondiasis`), `:22` (`Key` →
    /// `Disease-Chrondiasis`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:234` (row 15). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Chrondiasis",
        0,
        MagicSkill::None,
        &[Payload::Disease(&CHRONDIASIS_DATA)],
    ),
    /// `Consumption.cs:16`, `:18` (`Diseases.Consumption`), `:22` (`Key` →
    /// `Disease-Consumption`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:223` (row 4). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Consumption",
        0,
        MagicSkill::None,
        &[Payload::Disease(&CONSUMPTION_DATA)],
    ),
    /// `Dementia.cs:16`, `:18` (`Diseases.Dementia`), `:22` (`Key` → `Disease-Dementia`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:233` (row 14).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Dementia",
        0,
        MagicSkill::None,
        &[Payload::Disease(&DEMENTIA_DATA)],
    ),
    /// `Leprosy.cs:16`, `:18` (`Diseases.Leprosy`), `:22` (`Key` → `Disease-Leprosy`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:228` (row 9).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Leprosy",
        0,
        MagicSkill::None,
        &[Payload::Disease(&LEPROSY_DATA)],
    ),
    /// `Plague.cs:16`, `:18` (`Diseases.Plague`), `:22` (`Key` → `Disease-Plague`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:220` (row 1).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-Plague",
        0,
        MagicSkill::None,
        &[Payload::Disease(&PLAGUE_DATA)],
    ),
    /// `RedDeath.cs:16`, `:18` (`Diseases.RedDeath`), `:22` (`Key` → `Disease-RedDeath`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:230` (row 11).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-RedDeath",
        0,
        MagicSkill::None,
        &[Payload::Disease(&RED_DEATH_DATA)],
    ),
    /// `StomachRot.cs:16`, `:18` (`Diseases.StomachRot`), `:22` (`Key` →
    /// `Disease-StomachRot`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:222` (row 3). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-StomachRot",
        0,
        MagicSkill::None,
        &[Payload::Disease(&STOMACH_ROT_DATA)],
    ),
    /// `SwampRot.cs:16`, `:18` (`Diseases.SwampRot`), `:22` (`Key` → `Disease-SwampRot`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:225` (row 6).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-SwampRot",
        0,
        MagicSkill::None,
        &[Payload::Disease(&SWAMP_ROT_DATA)],
    ),
    /// `TyphoidFever.cs:16`, `:18` (`Diseases.TyphoidFever`), `:22` (`Key` →
    /// `Disease-TyphoidFever`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:232` (row 13). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-TyphoidFever",
        0,
        MagicSkill::None,
        &[Payload::Disease(&TYPHOID_FEVER_DATA)],
    ),
    /// `VampirismInfection.cs:33` (`class VampirismInfection : DiseaseEffect`), `:35`
    /// (`VampirismInfectionKey = "Vampirism-Infection"`), `:46` (`properties.Key =
    /// VampirismInfectionKey`), `:48` (`classicDiseaseType = Diseases.None`), `:49`
    /// (`diseaseData = new DiseaseData(…0, 0xFF, 0xFF)`), `:50`
    /// (`bypassSavingThrows = true` — a flag of the effect, not of the properties).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Vampirism-Infection",
        0,
        MagicSkill::None,
        &[Payload::Disease(&VAMPIRISM_INFECTION_DATA)],
    ),
    /// `WereboarInfection.cs:21` (`class WereboarInfection : LycanthropyInfection`),
    /// `:23` (`WereboarInfectionKey = "Wereboar-Infection"`), `:27`
    /// (`base.SetProperties()` → `LycanthropyInfection.cs:29-35`, own key skipped),
    /// `:28` (`properties.Key = WereboarInfectionKey`); bytes at
    /// `LycanthropyInfection.cs:33` (all zeros, `0xFF`/`0xFF`, permanent).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Wereboar-Infection",
        0,
        MagicSkill::None,
        &[Payload::Disease(&LYCANTHROPY_INFECTION_DATA)],
    ),
    /// `WerewolfInfection.cs:21` (`class WerewolfInfection : LycanthropyInfection`),
    /// `:23` (`WerewolfInfectionKey = "Werewolf-Infection"`), `:27`
    /// (`base.SetProperties()`), `:28` (`properties.Key = WerewolfInfectionKey`);
    /// bytes at `LycanthropyInfection.cs:33` (all zeros, `0xFF`/`0xFF`, permanent).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Werewolf-Infection",
        0,
        MagicSkill::None,
        &[Payload::Disease(&LYCANTHROPY_INFECTION_DATA)],
    ),
    /// `WitchesPox.cs:16`, `:18` (`Diseases.WitchesPox`), `:22` (`Key` →
    /// `Disease-WitchesPox`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:219` (row 0). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-WitchesPox",
        0,
        MagicSkill::None,
        &[Payload::Disease(&WITCHES_POX_DATA)],
    ),
    /// `WizardFever.cs:16`, `:18` (`Diseases.WizardFever`), `:22` (`Key` →
    /// `Disease-WizardFever`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:235` (row 16). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-WizardFever",
        0,
        MagicSkill::None,
        &[Payload::Disease(&WIZARD_FEVER_DATA)],
    ),
    /// `WoundRot.cs:16`, `:18` (`Diseases.WoundRot`), `:22` (`Key` → `Disease-WoundRot`),
    /// `:25` (`GetClassicDiseaseData`); bytes at `DiseaseEffect.cs:229` (row 10).
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-WoundRot",
        0,
        MagicSkill::None,
        &[Payload::Disease(&WOUND_ROT_DATA)],
    ),
    /// `YellowFever.cs:16`, `:18` (`Diseases.YellowFever`), `:22` (`Key` →
    /// `Disease-YellowFever`), `:25` (`GetClassicDiseaseData`); bytes at
    /// `DiseaseEffect.cs:221` (row 2). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Disease-YellowFever",
        0,
        MagicSkill::None,
        &[Payload::Disease(&YELLOW_FEVER_DATA)],
    ),
];
