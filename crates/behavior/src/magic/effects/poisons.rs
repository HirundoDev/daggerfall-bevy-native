//! Poisons family — 12 registered templates from DFU `0038a1e6`
//! (`Assets/Scripts/Game/MagicAndEffects/Effects/Poisons/PoisonEffect.cs`).
//!
//! Census: **1** `.cs` file, **0** abstract bases, **12** variants registered in
//! `SetProperties()` (`PoisonEffect.cs:85-100`: `variantCount = totalVariants` at
//! `:87`, `totalVariants = 12` at `:26`, one `SetVariantProperties` call per line
//! `:88-99`) ⇒ **12** rows, one per variant, in registration order. This matches
//! the `Poisons` enum (`ItemEnums.cs:155-170`, `None = -1`, then `Nux_Vomica = 128`
//! … `Aegrotat = 139`; `startValue = 128` at `PoisonEffect.cs:25`).
//!
//! Each row is one variant of the same class:
//! * `Key = GetClassicPoisonEffectKey(poisonType)` =
//!   `string.Format("Poison-{0}", poisonType.ToString())` (`PoisonEffect.cs:166-169`),
//!   assigned in `SetVariantProperties` (`:295-304`, `Key` at `:300`);
//! * `properties.ShowSpellIcon = false` (`:301`, not modeled by `EffectProperties`);
//! * **no** `properties.ClassicKey` ⇒ `classic_key` is `0` (contract §3);
//! * **no** `properties.MagicSkill` ⇒ `MagicSkill::None` (`EntityEffect.cs:302`
//!   default); poisons are not spell schools;
//! * **no** `SupportDuration` / `SupportChance` / `SupportMagnitude` and no cost
//!   block (`Start` rolls the counters itself, `:137-144`) ⇒ plain
//!   `EffectProperties::new` plus `.variants(12)` for the declared `variantCount`.
//!
//! Incubation and duration come from the four arrays at `PoisonEffect.cs:137-140`,
//! indexed `(int)PoisonType - startValue` (`:142`), rolled as
//! `Random.Range(min, max + 1)` (`:143-144`) — i.e. **inclusive** `[min, max]`,
//! which is exactly `PoisonSpec::minutes_to_start` / `minutes_of_poison`.
//!
//! The per-minute payload is the `switch (PoisonType)` at
//! `PoisonEffect.cs:228-286` (`IncrementPoisonEffects`, `:215-293`). Every
//! `Random.Range(a, b)` there is half-open `[a, b)`, so the stored max is `b - 1`;
//! fixed calls store `(n, n)`. `sign` is −1 for the damages and +1 for the drug
//! buffs. Resource rows carry `scale`: the two fatigue calls flagged
//! `assignMultiplier = true` (`:250`, `:268`, `:279`) multiply the roll by
//! `DaggerfallEntity.FatigueMultiplier = 64` (`DaggerfallEntity.cs:39`, applied at
//! `:345`/`:355`) ⇒ `scale = 64`; every other resource call uses `scale = 1`.
//!
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md`
//! (§2 vocabulary, §3 builder, §4.1 Poisons mapping).
//!
//! Each row cites its source `file:line` on the `///` comment immediately above it.
//! A doc comment on an expression (array element) is ignored by rustdoc rather than
//! shown, so `unused_doc_comments` is allowed module-wide to keep the build
//! warning-free.
#![allow(unused_doc_comments)]

use crate::magic::{
    Attribute, EffectProperties, MagicSkill, Payload, PoisonSpec, PoisonTick, Resource,
};

/// `PoisonEffect.cs:137-140` column 0 — variant `Poisons.Nux_Vomica`
/// (`ItemEnums.cs:158`, `PoisonEffect.cs:88`). Incubation `(4, 4)`, duration `(3, 10)`;
/// tick `:230-232`: `DecreaseHealth(Random.Range(2, 12))` ⇒ health `2..=11`, sign −1.
const NUX_VOMICA_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (4, 4),
    minutes_of_poison: (3, 10),
    tick: PoisonTick {
        attributes: &[],
        resources: &[(Resource::Health, 2, 11, -1, 1)],
    },
};

/// `PoisonEffect.cs:137-140` column 1 — variant `Poisons.Arsenic`
/// (`ItemEnums.cs:159`, `PoisonEffect.cs:89`). Incubation `(10, 10)`, duration `(20, 1000)`;
/// tick `:234-237`: `DecreaseHealth(2)` fixed, `ChangeStatMod(Endurance, -1)` fixed.
const ARSENIC_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (10, 10),
    minutes_of_poison: (20, 1000),
    tick: PoisonTick {
        attributes: &[(Attribute::Endurance, 1, 1, -1)],
        resources: &[(Resource::Health, 2, 2, -1, 1)],
    },
};

/// `PoisonEffect.cs:137-140` column 2 — variant `Poisons.Moonseed`
/// (`ItemEnums.cs:160`, `PoisonEffect.cs:90`). Incubation `(0, 0)`, duration `(1, 4)`;
/// tick `:239-241`: `DecreaseHealth(Random.Range(1, 10))` ⇒ health `1..=9`, sign −1.
const MOONSEED_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (0, 0),
    minutes_of_poison: (1, 4),
    tick: PoisonTick {
        attributes: &[],
        resources: &[(Resource::Health, 1, 9, -1, 1)],
    },
};

/// `PoisonEffect.cs:137-140` column 3 — variant `Poisons.Drothweed`
/// (`ItemEnums.cs:161`, `PoisonEffect.cs:91`). Incubation `(5, 10)`, duration `(5, 30)`;
/// tick `:243-247`: Strength `-Random.Range(5, 10)` ⇒ `5..=9`, Agility and Speed
/// `-Random.Range(1, 5)` ⇒ `1..=4`, all sign −1; no resource row.
const DROTHWEED_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (5, 10),
    minutes_of_poison: (5, 30),
    tick: PoisonTick {
        attributes: &[
            (Attribute::Strength, 5, 9, -1),
            (Attribute::Agility, 1, 4, -1),
            (Attribute::Speed, 1, 4, -1),
        ],
        resources: &[],
    },
};

/// `PoisonEffect.cs:137-140` column 4 — variant `Poisons.Somnalius`
/// (`ItemEnums.cs:162`, `PoisonEffect.cs:92`). Incubation `(0, 0)`, duration `(2, 10)`;
/// tick `:249-251`: `DecreaseFatigue(Random.Range(10, 100), true)` ⇒ fatigue `10..=99`,
/// sign −1, `assignMultiplier = true` ⇒ `scale = 64`.
const SOMNALIUS_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (0, 0),
    minutes_of_poison: (2, 10),
    tick: PoisonTick {
        attributes: &[],
        resources: &[(Resource::Fatigue, 10, 99, -1, 64)],
    },
};

/// `PoisonEffect.cs:137-140` column 5 — variant `Poisons.Pyrrhic_Acid`
/// (`ItemEnums.cs:163`, `PoisonEffect.cs:93`). Incubation `(0, 0)`, duration `(1, 2)`;
/// tick `:253-255`: `DecreaseHealth(Random.Range(1, 30))` ⇒ health `1..=29`, sign −1.
const PYRRHIC_ACID_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (0, 0),
    minutes_of_poison: (1, 2),
    tick: PoisonTick {
        attributes: &[],
        resources: &[(Resource::Health, 1, 29, -1, 1)],
    },
};

/// `PoisonEffect.cs:137-140` column 6 — variant `Poisons.Magebane`
/// (`ItemEnums.cs:164`, `PoisonEffect.cs:94`). Incubation `(2, 2)`, duration `(5, 20)`;
/// tick `:257-260`: Willpower `-Random.Range(1, 5)` ⇒ `1..=4`, sign −1;
/// `DecreaseMagicka(Random.Range(5, 15))` ⇒ spell points `5..=14`, sign −1.
const MAGEBANE_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (2, 2),
    minutes_of_poison: (5, 20),
    tick: PoisonTick {
        attributes: &[(Attribute::Willpower, 1, 4, -1)],
        resources: &[(Resource::SpellPoints, 5, 14, -1, 1)],
    },
};

/// `PoisonEffect.cs:137-140` column 7 — variant `Poisons.Thyrwort`
/// (`ItemEnums.cs:165`, `PoisonEffect.cs:95`). Incubation `(0, 0)`, duration `(1, 3)`;
/// tick `:262-265`: Willpower `-Random.Range(5, 20)` ⇒ `5..=19`, Personality
/// `-Random.Range(10, 20)` ⇒ `10..=19`, both sign −1; no resource row.
const THYRWORT_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (0, 0),
    minutes_of_poison: (1, 3),
    tick: PoisonTick {
        attributes: &[(Attribute::Willpower, 5, 19, -1), (Attribute::Personality, 10, 19, -1)],
        resources: &[],
    },
};

/// `PoisonEffect.cs:137-140` column 8 — drug variant `Poisons.Indulcet`
/// (`ItemEnums.cs:166`, `PoisonEffect.cs:96`; drug per `:320-323`). Incubation `(2, 12)`,
/// duration `(2, 6)`; tick `:267-270`: `DecreaseFatigue(Random.Range(10, 100), true)`
/// ⇒ fatigue `10..=99`, sign −1, `scale = 64`; Luck `Random.Range(4, 10)` ⇒ `4..=9`,
/// sign +1.
const INDULCET_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (2, 12),
    minutes_of_poison: (2, 6),
    tick: PoisonTick {
        attributes: &[(Attribute::Luck, 4, 9, 1)],
        resources: &[(Resource::Fatigue, 10, 99, -1, 64)],
    },
};

/// `PoisonEffect.cs:137-140` column 9 — drug variant `Poisons.Sursum`
/// (`ItemEnums.cs:167`, `PoisonEffect.cs:97`). Incubation `(1, 4)`, duration `(2, 2)`;
/// tick `:272-275`: Intelligence `-Random.Range(10, 30)` ⇒ `10..=29`, sign −1;
/// Strength `Random.Range(5, 20)` ⇒ `5..=19`, sign +1.
const SURSUM_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (1, 4),
    minutes_of_poison: (2, 2),
    tick: PoisonTick {
        attributes: &[(Attribute::Intelligence, 10, 29, -1), (Attribute::Strength, 5, 19, 1)],
        resources: &[],
    },
};

/// `PoisonEffect.cs:137-140` column 10 — drug variant `Poisons.Quaesto_Vil`
/// (`ItemEnums.cs:168`, `PoisonEffect.cs:98`). Incubation `(2, 12)`, duration `(1, 4)`;
/// tick `:277-280`: Willpower `-Random.Range(1, 4)` ⇒ `1..=3`, sign −1;
/// `IncreaseFatigue(Random.Range(5, 10), true)` ⇒ fatigue `5..=9`, sign +1, `scale = 64`.
const QUAESTO_VIL_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (2, 12),
    minutes_of_poison: (1, 4),
    tick: PoisonTick {
        attributes: &[(Attribute::Willpower, 1, 3, -1)],
        resources: &[(Resource::Fatigue, 5, 9, 1, 64)],
    },
};

/// `PoisonEffect.cs:137-140` column 11 — drug variant `Poisons.Aegrotat`
/// (`ItemEnums.cs:169`, `PoisonEffect.cs:99`). Incubation `(0, 0)`, duration `(5, 20)`;
/// tick `:282-285`: Endurance `-Random.Range(1, 5)` ⇒ `1..=4`, sign −1;
/// `IncreaseMagicka(Random.Range(5, 10))` ⇒ spell points `5..=9`, sign +1.
const AEGROTAT_SPEC: PoisonSpec = PoisonSpec {
    minutes_to_start: (0, 0),
    minutes_of_poison: (5, 20),
    tick: PoisonTick {
        attributes: &[(Attribute::Endurance, 1, 4, -1)],
        resources: &[(Resource::SpellPoints, 5, 9, 1, 1)],
    },
};

/// The 12 Poison variants of DFU `0038a1e6`, in `SetProperties()` registration order
/// (`PoisonEffect.cs:88-99`).
pub const POISONS: &[EffectProperties] = &[
    /// `PoisonEffect.cs:88` registers variant 0 (`Poisons.Nux_Vomica`); `Key` from
    /// `GetClassicPoisonEffectKey` (`:166-169`, `"Poison-{0}"` via `ToString()`),
    /// assigned at `:300`. Incubation/duration `:137-140` col 0; tick `:230-232`.
    /// `variantCount = 12` (`:87`, `:26`). No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Nux_Vomica",
        0,
        MagicSkill::None,
        &[Payload::Poison(&NUX_VOMICA_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:89` registers variant 1 (`Poisons.Arsenic`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 1; tick `:234-237`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Arsenic",
        0,
        MagicSkill::None,
        &[Payload::Poison(&ARSENIC_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:90` registers variant 2 (`Poisons.Moonseed`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 2; tick `:239-241`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Moonseed",
        0,
        MagicSkill::None,
        &[Payload::Poison(&MOONSEED_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:91` registers variant 3 (`Poisons.Drothweed`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 3; tick `:243-247`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Drothweed",
        0,
        MagicSkill::None,
        &[Payload::Poison(&DROTHWEED_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:92` registers variant 4 (`Poisons.Somnalius`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 4; tick `:249-251`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Somnalius",
        0,
        MagicSkill::None,
        &[Payload::Poison(&SOMNALIUS_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:93` registers variant 5 (`Poisons.Pyrrhic_Acid`); `Key` assigned
    /// at `:300` (`ToString()` keeps the underscore: `Poison-Pyrrhic_Acid`).
    /// Incubation/duration `:137-140` col 5; tick `:253-255`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Pyrrhic_Acid",
        0,
        MagicSkill::None,
        &[Payload::Poison(&PYRRHIC_ACID_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:94` registers variant 6 (`Poisons.Magebane`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 6; tick `:257-260`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Magebane",
        0,
        MagicSkill::None,
        &[Payload::Poison(&MAGEBANE_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:95` registers variant 7 (`Poisons.Thyrwort`); `Key` assigned at
    /// `:300`. Incubation/duration `:137-140` col 7; tick `:262-265`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Thyrwort",
        0,
        MagicSkill::None,
        &[Payload::Poison(&THYRWORT_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:96` registers variant 8 (`Poisons.Indulcet`, drug); `Key`
    /// assigned at `:300`. Incubation/duration `:137-140` col 8; tick `:267-270`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Indulcet",
        0,
        MagicSkill::None,
        &[Payload::Poison(&INDULCET_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:97` registers variant 9 (`Poisons.Sursum`, drug); `Key` assigned
    /// at `:300`. Incubation/duration `:137-140` col 9; tick `:272-275`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Sursum",
        0,
        MagicSkill::None,
        &[Payload::Poison(&SURSUM_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:98` registers variant 10 (`Poisons.Quaesto_Vil`, drug); `Key`
    /// assigned at `:300` (`Poison-Quaesto_Vil`). Incubation/duration `:137-140` col 10;
    /// tick `:277-280`. No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Quaesto_Vil",
        0,
        MagicSkill::None,
        &[Payload::Poison(&QUAESTO_VIL_SPEC)],
    )
    .variants(12),
    /// `PoisonEffect.cs:99` registers variant 11 (`Poisons.Aegrotat`, drug); `Key`
    /// assigned at `:300`. Incubation/duration `:137-140` col 11; tick `:282-285`.
    /// No `ClassicKey`, no `MagicSkill`, no supports.
    EffectProperties::new(
        "Poison-Aegrotat",
        0,
        MagicSkill::None,
        &[Payload::Poison(&AEGROTAT_SPEC)],
    )
    .variants(12),
];
