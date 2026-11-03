//! M41 W41-3 — complete magic: `PROBE-SYS-MAGIC-001`.
//!
//! Contract: `docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md`
//! (registry census §1/§4, lifetime §2.1, per-template evidence §5) and
//! `docs/sintergic/game/programming/m23-magic-spec-CR-5243af63.md` §2.1
//! (rounds/chance goldens), §3.1/§3.2 (both classic cost routes), §3.3
//! (goldens D/E), §4 (range/target), §5 (spellmaker) and §6 N-4 (the record
//! `cost` field is written, never read).
//!
//! The probe extends `PROBE-EFFECT-001`/`PROBE-SPELLMAKER-001`
//! (`crates/app/src/m23_probe.rs`) — it never contradicts them: the same
//! registry, the same engine entry points (`cast_effects`,
//! `magic_round_with`, `run_enchantment`, `effect_cost`,
//! `calculate_total_effect_costs`, `calculate_casting_cost`, `SpellMaker`) and
//! the same 89-byte `SPELLS.STD` record layout of
//! `daggerfall_parser_rs::spells_std` (spec §1.2) — and adds the per-row
//! census this work package certifies.
//!
//! Blocks:
//! - **A** — one `SCHOOL` line per school and one `EFFECT` row per registered
//!   template (measured registry length, coverage driven through the engine;
//!   `NEGATIVE` only for a payload class the single-entity model declares
//!   residual, with the contract path that proves it).
//! - **B** — `ROUND` goldens of `base + plus · max(1, ⌊level / perLevel⌋)`.
//! - **C** — `COST` goldens of both classic cost routes: `formula=B`
//!   (`CalculateCastingCost`, spec §3.2) and `formula=C`
//!   (`CalculateTotalEffectCosts`, spec §3.1), plus a vector that changes the
//!   settings and the cost with them (a formula, not a table).
//! - **D** — `RANGE`/`RANGE-MOD` goldens for the five classic `rangeType`
//!   values and their cost multipliers, per route.
//! - **E** — a 3-effect `SpellMaker` spell (cost, school split, name) and its
//!   `SPELLPERSIST` roundtrip through the record layout + the real parser.
//! - **F** — adversarial refusals: unknown effect key, 4th spellmaker slot,
//!   magnitude/cost overflow (saturating, never wrapping) and a zero-duration
//!   buff that must not become permanent.
//!
//! Determinism: fixed dice, no clock, no I/O, no retail data. Two independent
//! processes must print the same bytes; `PROBE_RUN` is never read nor printed.
//! The last line is `DIGEST <sha256>` = sha256 of every line printed before it
//! (joined with `\n`, no trailing newline).

use std::collections::VecDeque;

use daggerfall_behavior::magic::{
    apply_target_multiplier, calculate_casting_cost, calculate_total_effect_costs, chance_value,
    classic_effect_entry, classic_key, component_cost, effect_by_classic_key, effect_by_key,
    effect_cost, enchant_pt_cost, make_effect_costs, resolve_classic_effects,
    reverse_classic_key, roll_magnitude, rounds, run_enchantment, templates, EffectDice,
    EffectEntry, EffectProperties, EffectSettings, EffectTarget, Gate, MagicSkill, MagicState,
    Payload, SpellBundle, SpellMaker, SpellmakerError, TargetType, MAX_EFFECTS_PER_SPELL,
    TEMPLATE_COUNT,
};
use daggerfall_parser_rs::spells_std::{
    parse_spells_std, EffectSlot, CHANCE_OFFSET, COST_OFFSET, DURATION_OFFSET,
    EFFECT_SLOT_COUNT, EFFECT_TYPES_OFFSET, ELEMENT_OFFSET, ICON_OFFSET, MAGNITUDE_OFFSET,
    RANGE_TYPE_OFFSET, SPELL_INDEX_OFFSET, SPELL_NAME_BYTES, SPELL_NAME_OFFSET,
    SPELL_RECORD_SIZE,
};

/// Lane correlation id.
const CR: &str = "CR-a86cec9d-3ec1-406f-aac9-82d2265bbb28";


// ── citations into the engine (measured after the W41-3 fixes) ──────────────

const SRC_ROUNDS: &str = "crates/behavior/src/magic.rs:84";
const SRC_CHANCE: &str = "crates/behavior/src/magic.rs:107";
const SRC_ROLL_MAGNITUDE: &str = "crates/behavior/src/magic.rs:1862";
const SRC_PERMANENT: &str = "crates/behavior/src/magic.rs:2901";
const SRC_TEMPLATE_COUNT: &str = "crates/behavior/src/magic.rs:2158";
const SRC_TEMPLATES: &str = "crates/behavior/src/magic.rs:2170";
const SRC_RESOLVE: &str = "crates/behavior/src/magic.rs:2301";
const SRC_COMPONENT_COST: &str = "crates/behavior/src/magic.rs:2323";
const SRC_EFFECT_COST: &str = "crates/behavior/src/magic.rs:2342";
const SRC_TOTAL_COST: &str = "crates/behavior/src/magic.rs:2464";
const SRC_CASTING_COST: &str = "crates/behavior/src/magic.rs:2677";
const SRC_RANGE_MULT: &str = "crates/behavior/src/magic.rs:2439";
const SRC_RANGE_PARSE: &str = "crates/behavior/src/magic.rs:2425";
const SRC_SPELLMAKER: &str = "crates/behavior/src/magic.rs:3772";
const SRC_SLOT_CAP: &str = "crates/behavior/src/magic.rs:3759";
const SRC_SPELL_BUNDLE: &str = "crates/behavior/src/magic.rs:3676";
const SRC_ENCHANT_PT: &str = "crates/behavior/src/magic.rs:2732";
const SRC_RECORD_LAYOUT: &str = "crates/daggerfall_parser_rs/src/spells_std.rs:109";

/// One registry row's citation, measured from the family files. The runner
/// re-reads every path and fails if the line does not open
/// `EffectProperties::new(` for that key (a stale table is a red gate).
const EFFECT_SOURCES: &[(&str, &str)] = &[
    ("Climbing", "crates/behavior/src/magic/effects/alteration.rs:28"),
    ("ElementalResistance-Fire", "crates/behavior/src/magic/effects/alteration.rs:41"),
    ("ElementalResistance-Frost", "crates/behavior/src/magic/effects/alteration.rs:54"),
    ("ElementalResistance-Poison", "crates/behavior/src/magic/effects/alteration.rs:68"),
    ("ElementalResistance-Shock", "crates/behavior/src/magic/effects/alteration.rs:81"),
    ("ElementalResistance-Magicka", "crates/behavior/src/magic/effects/alteration.rs:95"),
    ("Jumping", "crates/behavior/src/magic/effects/alteration.rs:108"),
    ("Paralyze", "crates/behavior/src/magic/effects/alteration.rs:119"),
    ("Shield", "crates/behavior/src/magic/effects/alteration.rs:130"),
    ("Slowfall", "crates/behavior/src/magic/effects/alteration.rs:141"),
    ("WaterBreathing", "crates/behavior/src/magic/effects/alteration.rs:151"),
    ("ContinuousDamage-Fatigue", "crates/behavior/src/magic/effects/destruction.rs:42"),
    ("ContinuousDamage-Health", "crates/behavior/src/magic/effects/destruction.rs:54"),
    ("ContinuousDamage-SpellPoints", "crates/behavior/src/magic/effects/destruction.rs:66"),
    ("Damage-Fatigue", "crates/behavior/src/magic/effects/destruction.rs:78"),
    ("Damage-Health", "crates/behavior/src/magic/effects/destruction.rs:89"),
    ("Damage-SpellPoints", "crates/behavior/src/magic/effects/destruction.rs:100"),
    ("Disintegrate", "crates/behavior/src/magic/effects/destruction.rs:111"),
    ("Drain-Agility", "crates/behavior/src/magic/effects/destruction.rs:119"),
    ("Drain-Endurance", "crates/behavior/src/magic/effects/destruction.rs:127"),
    ("Drain-Intelligence", "crates/behavior/src/magic/effects/destruction.rs:135"),
    ("Drain-Luck", "crates/behavior/src/magic/effects/destruction.rs:143"),
    ("Drain-Personality", "crates/behavior/src/magic/effects/destruction.rs:151"),
    ("Drain-Speed", "crates/behavior/src/magic/effects/destruction.rs:159"),
    ("Drain-Strength", "crates/behavior/src/magic/effects/destruction.rs:167"),
    ("Drain-Willpower", "crates/behavior/src/magic/effects/destruction.rs:175"),
    ("Transfer-Agility", "crates/behavior/src/magic/effects/destruction.rs:184"),
    ("Transfer-Endurance", "crates/behavior/src/magic/effects/destruction.rs:196"),
    ("Transfer-Fatigue", "crates/behavior/src/magic/effects/destruction.rs:208"),
    ("Transfer-Health", "crates/behavior/src/magic/effects/destruction.rs:217"),
    ("Transfer-Intelligence", "crates/behavior/src/magic/effects/destruction.rs:226"),
    ("Transfer-Luck", "crates/behavior/src/magic/effects/destruction.rs:238"),
    ("Transfer-Personality", "crates/behavior/src/magic/effects/destruction.rs:250"),
    ("Transfer-Speed", "crates/behavior/src/magic/effects/destruction.rs:262"),
    ("Transfer-Strength", "crates/behavior/src/magic/effects/destruction.rs:274"),
    ("Transfer-Willpower", "crates/behavior/src/magic/effects/destruction.rs:286"),
    ("Disease-BloodRot", "crates/behavior/src/magic/effects/diseases.rs:435"),
    ("Disease-BrainFever", "crates/behavior/src/magic/effects/diseases.rs:444"),
    ("Disease-CalironsCurse", "crates/behavior/src/magic/effects/diseases.rs:453"),
    ("Disease-Cholera", "crates/behavior/src/magic/effects/diseases.rs:462"),
    ("Disease-Chrondiasis", "crates/behavior/src/magic/effects/diseases.rs:471"),
    ("Disease-Consumption", "crates/behavior/src/magic/effects/diseases.rs:480"),
    ("Disease-Dementia", "crates/behavior/src/magic/effects/diseases.rs:489"),
    ("Disease-Leprosy", "crates/behavior/src/magic/effects/diseases.rs:498"),
    ("Disease-Plague", "crates/behavior/src/magic/effects/diseases.rs:507"),
    ("Disease-RedDeath", "crates/behavior/src/magic/effects/diseases.rs:516"),
    ("Disease-StomachRot", "crates/behavior/src/magic/effects/diseases.rs:525"),
    ("Disease-SwampRot", "crates/behavior/src/magic/effects/diseases.rs:534"),
    ("Disease-TyphoidFever", "crates/behavior/src/magic/effects/diseases.rs:543"),
    ("Vampirism-Infection", "crates/behavior/src/magic/effects/diseases.rs:555"),
    ("Wereboar-Infection", "crates/behavior/src/magic/effects/diseases.rs:567"),
    ("Werewolf-Infection", "crates/behavior/src/magic/effects/diseases.rs:578"),
    ("Disease-WitchesPox", "crates/behavior/src/magic/effects/diseases.rs:587"),
    ("Disease-WizardFever", "crates/behavior/src/magic/effects/diseases.rs:596"),
    ("Disease-WoundRot", "crates/behavior/src/magic/effects/diseases.rs:605"),
    ("Disease-YellowFever", "crates/behavior/src/magic/effects/diseases.rs:614"),
    ("AbsorbsSpells", "crates/behavior/src/magic/effects/enchanting.rs:41"),
    ("BadReactionsFrom", "crates/behavior/src/magic/effects/enchanting.rs:53"),
    ("BadRepWith", "crates/behavior/src/magic/effects/enchanting.rs:67"),
    ("CastWhenHeld", "crates/behavior/src/magic/effects/enchanting.rs:79"),
    ("CastWhenStrikes", "crates/behavior/src/magic/effects/enchanting.rs:96"),
    ("CastWhenUsed", "crates/behavior/src/magic/effects/enchanting.rs:107"),
    ("EnhancesSkill", "crates/behavior/src/magic/effects/enchanting.rs:119"),
    ("ExtraSpellPts", "crates/behavior/src/magic/effects/enchanting.rs:167"),
    ("ExtraWeight", "crates/behavior/src/magic/effects/enchanting.rs:178"),
    ("FeatherWeight", "crates/behavior/src/magic/effects/enchanting.rs:190"),
    ("GoodRepWith", "crates/behavior/src/magic/effects/enchanting.rs:201"),
    ("HealthLeech", "crates/behavior/src/magic/effects/enchanting.rs:214"),
    ("ImprovesTalents", "crates/behavior/src/magic/effects/enchanting.rs:256"),
    ("IncreasedWeightAllowance", "crates/behavior/src/magic/effects/enchanting.rs:271"),
    ("ItemDeteriorates", "crates/behavior/src/magic/effects/enchanting.rs:287"),
    ("LowDamageVs", "crates/behavior/src/magic/effects/enchanting.rs:299"),
    ("PotentVs", "crates/behavior/src/magic/effects/enchanting.rs:328"),
    ("RegensHealth", "crates/behavior/src/magic/effects/enchanting.rs:357"),
    ("RepairsObjects", "crates/behavior/src/magic/effects/enchanting.rs:373"),
    ("SoulBound", "crates/behavior/src/magic/effects/enchanting.rs:385"),
    ("StrengthensArmor", "crates/behavior/src/magic/effects/enchanting.rs:396"),
    ("UserTakesDamage", "crates/behavior/src/magic/effects/enchanting.rs:407"),
    ("VampiricEffect", "crates/behavior/src/magic/effects/enchanting.rs:428"),
    ("WeakensArmor", "crates/behavior/src/magic/effects/enchanting.rs:453"),
    ("Chameleon-Normal", "crates/behavior/src/magic/effects/illusion.rs:29"),
    ("Chameleon-True", "crates/behavior/src/magic/effects/illusion.rs:41"),
    ("Invisibility-Normal", "crates/behavior/src/magic/effects/illusion.rs:53"),
    ("Invisibility-True", "crates/behavior/src/magic/effects/illusion.rs:65"),
    ("Light", "crates/behavior/src/magic/effects/illusion.rs:78"),
    ("MageLight-Inferno", "crates/behavior/src/magic/effects/illusion.rs:91"),
    ("MageLight-Rime", "crates/behavior/src/magic/effects/illusion.rs:102"),
    ("MageLight-Venom", "crates/behavior/src/magic/effects/illusion.rs:113"),
    ("MageLight-Storm", "crates/behavior/src/magic/effects/illusion.rs:124"),
    ("MageLight-Arcane", "crates/behavior/src/magic/effects/illusion.rs:135"),
    ("MorphSelf", "crates/behavior/src/magic/effects/illusion.rs:148"),
    ("Shadow-Normal", "crates/behavior/src/magic/effects/illusion.rs:159"),
    ("Shadow-True", "crates/behavior/src/magic/effects/illusion.rs:171"),
    ("ComprehendLanguages", "crates/behavior/src/magic/effects/mysticism.rs:21"),
    ("CreateItem", "crates/behavior/src/magic/effects/mysticism.rs:36"),
    ("Dispel-Daedra", "crates/behavior/src/magic/effects/mysticism.rs:50"),
    ("Dispel-Magic", "crates/behavior/src/magic/effects/mysticism.rs:65"),
    ("Dispel-Undead", "crates/behavior/src/magic/effects/mysticism.rs:80"),
    ("Lock", "crates/behavior/src/magic/effects/mysticism.rs:95"),
    ("Open", "crates/behavior/src/magic/effects/mysticism.rs:108"),
    ("Silence", "crates/behavior/src/magic/effects/mysticism.rs:122"),
    ("SoulTrap", "crates/behavior/src/magic/effects/mysticism.rs:138"),
    ("Teleport-Effect", "crates/behavior/src/magic/effects/mysticism.rs:152"),
    ("Poison-Nux_Vomica", "crates/behavior/src/magic/effects/poisons.rs:214"),
    ("Poison-Arsenic", "crates/behavior/src/magic/effects/poisons.rs:224"),
    ("Poison-Moonseed", "crates/behavior/src/magic/effects/poisons.rs:234"),
    ("Poison-Drothweed", "crates/behavior/src/magic/effects/poisons.rs:244"),
    ("Poison-Somnalius", "crates/behavior/src/magic/effects/poisons.rs:254"),
    ("Poison-Pyrrhic_Acid", "crates/behavior/src/magic/effects/poisons.rs:265"),
    ("Poison-Magebane", "crates/behavior/src/magic/effects/poisons.rs:275"),
    ("Poison-Thyrwort", "crates/behavior/src/magic/effects/poisons.rs:285"),
    ("Poison-Indulcet", "crates/behavior/src/magic/effects/poisons.rs:295"),
    ("Poison-Sursum", "crates/behavior/src/magic/effects/poisons.rs:305"),
    ("Poison-Quaesto_Vil", "crates/behavior/src/magic/effects/poisons.rs:315"),
    ("Poison-Aegrotat", "crates/behavior/src/magic/effects/poisons.rs:325"),
    ("Cure-Disease", "crates/behavior/src/magic/effects/restoration.rs:27"),
    ("Cure-Paralyzation", "crates/behavior/src/magic/effects/restoration.rs:37"),
    ("Cure-Poison", "crates/behavior/src/magic/effects/restoration.rs:46"),
    ("Fortify-Agility", "crates/behavior/src/magic/effects/restoration.rs:57"),
    ("Fortify-Endurance", "crates/behavior/src/magic/effects/restoration.rs:68"),
    ("Fortify-Intelligence", "crates/behavior/src/magic/effects/restoration.rs:80"),
    ("Fortify-Luck", "crates/behavior/src/magic/effects/restoration.rs:91"),
    ("Fortify-Personality", "crates/behavior/src/magic/effects/restoration.rs:103"),
    ("Fortify-Speed", "crates/behavior/src/magic/effects/restoration.rs:114"),
    ("Fortify-Strength", "crates/behavior/src/magic/effects/restoration.rs:125"),
    ("Fortify-Willpower", "crates/behavior/src/magic/effects/restoration.rs:137"),
    ("FreeAction", "crates/behavior/src/magic/effects/restoration.rs:148"),
    ("Heal-Agility", "crates/behavior/src/magic/effects/restoration.rs:159"),
    ("Heal-Endurance", "crates/behavior/src/magic/effects/restoration.rs:169"),
    ("Heal-Fatigue", "crates/behavior/src/magic/effects/restoration.rs:179"),
    ("Heal-Health", "crates/behavior/src/magic/effects/restoration.rs:192"),
    ("Heal-Intelligence", "crates/behavior/src/magic/effects/restoration.rs:205"),
    ("Heal-Luck", "crates/behavior/src/magic/effects/restoration.rs:214"),
    ("Heal-Personality", "crates/behavior/src/magic/effects/restoration.rs:224"),
    ("Heal-Speed", "crates/behavior/src/magic/effects/restoration.rs:234"),
    ("Heal-SpellPoints", "crates/behavior/src/magic/effects/restoration.rs:247"),
    ("Heal-Strength", "crates/behavior/src/magic/effects/restoration.rs:260"),
    ("Heal-Willpower", "crates/behavior/src/magic/effects/restoration.rs:270"),
    ("Regenerate", "crates/behavior/src/magic/effects/restoration.rs:280"),
    ("SpellAbsorption", "crates/behavior/src/magic/effects/restoration.rs:295"),
    ("Azuras_Star", "crates/behavior/src/magic/effects/special.rs:42"),
    ("Lycanthropy-Curse", "crates/behavior/src/magic/effects/special.rs:66"),
    ("Mace_of_Molag_Bal", "crates/behavior/src/magic/effects/special.rs:105"),
    ("Masque_of_Clavicus", "crates/behavior/src/magic/effects/special.rs:130"),
    ("Mehrunes_Razor", "crates/behavior/src/magic/effects/special.rs:141"),
    ("Oghma_Infinium", "crates/behavior/src/magic/effects/special.rs:152"),
    ("Passive-Specials", "crates/behavior/src/magic/effects/special.rs:173"),
    ("Ring_of_Namira", "crates/behavior/src/magic/effects/special.rs:203"),
    ("Sanguine_Rose", "crates/behavior/src/magic/effects/special.rs:214"),
    ("Skull_of_Corruption", "crates/behavior/src/magic/effects/special.rs:225"),
    ("Vampirism-Curse", "crates/behavior/src/magic/effects/special.rs:242"),
    ("Wabbajack", "crates/behavior/src/magic/effects/special.rs:269"),
    ("Charm", "crates/behavior/src/magic/effects/thaumaturgy.rs:31"),
    ("Detect-Enemy", "crates/behavior/src/magic/effects/thaumaturgy.rs:44"),
    ("Detect-Magic", "crates/behavior/src/magic/effects/thaumaturgy.rs:57"),
    ("Detect-Treasure", "crates/behavior/src/magic/effects/thaumaturgy.rs:70"),
    ("Identify", "crates/behavior/src/magic/effects/thaumaturgy.rs:83"),
    ("Levitate", "crates/behavior/src/magic/effects/thaumaturgy.rs:97"),
    ("Pacify-Animal", "crates/behavior/src/magic/effects/thaumaturgy.rs:111"),
    ("Pacify-Undead", "crates/behavior/src/magic/effects/thaumaturgy.rs:123"),
    ("Pacify-Humanoid", "crates/behavior/src/magic/effects/thaumaturgy.rs:135"),
    ("Pacify-Daedra", "crates/behavior/src/magic/effects/thaumaturgy.rs:147"),
    ("SpellReflection", "crates/behavior/src/magic/effects/thaumaturgy.rs:162"),
    ("SpellResistance", "crates/behavior/src/magic/effects/thaumaturgy.rs:178"),
    ("WaterWalking", "crates/behavior/src/magic/effects/thaumaturgy.rs:193"),];

// ── row output ─────────────────────────────────────────────────────────────

/// Evidence rows: `[OK] <id> - <detail> (src=<path:line>)` on green,
/// `FAIL <id> got=<v> want=<v>` on red (the M19-M24 probe convention).
struct Out {
    lines: Vec<String>,
    failures: usize,
}

impl Out {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            failures: 0,
        }
    }

    fn evidence(&mut self, line: String) {
        self.lines.push(line);
    }

    /// A checked row: `got` and `want` compare as strings.
    fn check(&mut self, id: &str, got: String, want: String, detail: &str, src: &str) {
        if got == want {
            self.lines
                .push(format!("[OK] {id} - {detail} (src={src})"));
        } else {
            self.failures += 1;
            self.lines.push(format!("FAIL {id} got={got} want={want}"));
        }
    }

    fn check_n(&mut self, id: &str, got: i64, want: i64, detail: &str, src: &str) {
        self.check(id, got.to_string(), want.to_string(), detail, src);
    }

    fn check_bool(&mut self, id: &str, got: bool, want: bool, detail: &str, src: &str) {
        self.check(
            id,
            if got { "true" } else { "false" }.to_string(),
            if want { "true" } else { "false" }.to_string(),
            detail,
            src,
        );
    }

    /// Resolves an engine lookup into evidence: an absent registration is a
    /// red `FAIL` row that the runner exits on, never an `expect` abort
    /// (which would report exit 101 with no row at all).
    fn require<T>(&mut self, id: &str, want: &str, value: Option<T>) -> Option<T> {
        if value.is_none() {
            self.failures += 1;
            self.lines.push(format!("FAIL {id} got=absent want={want}"));
        }
        value
    }

    /// The slot-refusal branch of `add_effect` as evidence: a refusal is a red
    /// row, never an abort.
    fn slot(&mut self, id: &str, want: usize, result: Result<(), SpellmakerError>) -> bool {
        match result {
            Ok(_) => true,
            Err(err) => {
                self.failures += 1;
                self.lines
                    .push(format!("FAIL {id} got={err:?} want=slot-{want}"));
                false
            }
        }
    }
}

// ── deterministic dice ─────────────────────────────────────────────────────

/// Scripted dice: `Range` answers queued values (then the low bound, so a
/// base magnitude is 5 and a plus magnitude 0) and `Dice100` answers 0 (the
/// `OnCast` roll `0 < chance` succeeds for every chance ≥ 1,
/// `Dice100.cs:15-18`).
struct FixedDice {
    ranges: VecDeque<i32>,
    d100: i32,
}

impl FixedDice {
    fn new(ranges: &[i32]) -> Self {
        Self {
            ranges: ranges.iter().copied().collect(),
            d100: 0,
        }
    }
}

impl EffectDice for FixedDice {
    fn range_inclusive(&mut self, low: i32, _high: i32) -> i32 {
        self.ranges.pop_front().unwrap_or(low)
    }

    fn d100(&mut self) -> i32 {
        self.d100
    }
}

// ── fixtures ───────────────────────────────────────────────────────────────

/// Flat 100/100 target, all attributes 50 (`MaxFatigue = (50 + 50)·64`).
fn flat_target() -> EffectTarget {
    EffectTarget::new(100, 100, [50; 8])
}

/// Target with room in both directions and every world gate held, so a drain,
/// a heal, a cure, a wear and a gated damage are all observable.
fn neutral_target() -> EffectTarget {
    let mut target = flat_target();
    target.health = 50;
    target.spell_points = 50;
    target.fatigue = target.max_fatigue() / 2;
    target.attribute_damage = [20; 8];
    target.skill_damage = [20; 35];
    target.item.max_condition = 100;
    target.item.condition = 50;
    target.item.magnitude = 5;
    target.diseases = vec!["Disease-Legacy"];
    target.poisons = vec!["Poison-Legacy"];
    target.flags.set(daggerfall_behavior::magic::Flag::Paralyzed, true);
    for gate in Gate::ALL {
        target.gates.set(gate, true);
    }
    target
}

/// Settings that make every supported component observable at level 1:
/// duration `(1,1,1)` ⇒ 2 rounds, chance `(100,0,1)` ⇒ always passes,
/// magnitude `(5,5,0,0,1)` ⇒ 5.
fn coverage_settings() -> EffectSettings {
    EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        chance_base: 100,
        chance_plus: 0,
        chance_per_level: 1,
        magnitude_base_min: 5,
        magnitude_base_max: 5,
        magnitude_plus_min: 0,
        magnitude_plus_max: 0,
        magnitude_per_level: 1,
    }
}

/// Duration settings over the classic defaults.
fn only_duration(base: i32, plus: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        duration_base: base,
        duration_plus: plus,
        duration_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// Chance settings over the classic defaults.
fn only_chance(base: i32, plus: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        chance_base: base,
        chance_plus: plus,
        chance_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// Magnitude settings over the classic defaults.
fn only_magnitude(min: i32, max: i32, plus_min: i32, plus_max: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        magnitude_base_min: min,
        magnitude_base_max: max,
        magnitude_plus_min: plus_min,
        magnitude_plus_max: plus_max,
        magnitude_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// Registry entry by key; the census already proved the key exists, so a
/// miss is a probe defect and not a spec error.
fn entry(key: &str, settings: EffectSettings) -> EffectEntry {
    EffectEntry {
        props: effect_by_key(key).unwrap_or_else(|| panic!("registry key missing: {key}")),
        settings,
    }
}

/// Classic slot (the parser's `EffectSlot` view) with no magnitude.
fn slot(effect_type: i8, sub_type: i8, duration: [u8; 3], chance: [u8; 3], magnitude: [u8; 5]) -> daggerfall_behavior::magic::ClassicEffect {
    daggerfall_behavior::magic::ClassicEffect {
        effect_type,
        sub_type,
        duration,
        chance,
        magnitude,
    }
}

// ── block A · per-school effect census ─────────────────────────────────────

/// Label of a magic school (`DFCareer.MagicSkills` via `career_skill_index`).
fn school_label(skill: MagicSkill) -> &'static str {
    match skill {
        MagicSkill::None => "None",
        MagicSkill::Destruction => "Destruction",
        MagicSkill::Restoration => "Restoration",
        MagicSkill::Illusion => "Illusion",
        MagicSkill::Alteration => "Alteration",
        MagicSkill::Thaumaturgy => "Thaumaturgy",
        MagicSkill::Mysticism => "Mysticism",
    }
}

/// The DFU class a row belongs to: a multi-variant family
/// (`variantCount > 1`, contract §1: `ElementalResistance` 5, `MageLight` 5,
/// `Pacify` 4, `Poison` 12) counts as **one** class, every other row is its
/// own class.
fn class_of(props: &EffectProperties) -> &'static str {
    if props.variant_count > 1 {
        props.key.split('-').next().unwrap_or(props.key)
    } else {
        props.key
    }
}

fn source_of(key: &str) -> &'static str {
    EFFECT_SOURCES
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, source)| *source)
        .unwrap_or("MISSING")
}

/// Whether any payload (recursively inside `On`) matches `predicate`.
fn any_payload(props: &EffectProperties, predicate: &dyn Fn(&Payload) -> bool) -> bool {
    fn walk(payloads: &[Payload], predicate: &dyn Fn(&Payload) -> bool) -> bool {
        payloads.iter().any(|payload| match payload {
            Payload::On { action, .. } => {
                predicate(payload) || walk(action, predicate)
            }
            Payload::Bearer(action) => predicate(payload) || walk(action, predicate),
            single => predicate(single),
        })
    }
    walk(props.payload, predicate)
}

/// The contract path that proves a `NEGATIVE` class, or `None` when the
/// template has no declared residual and must be a real application.
fn declared_residual(props: &EffectProperties) -> Option<(&'static str, &'static str)> {
    if any_payload(props, &|payload| matches!(payload, Payload::HealCaster(_))) {
        return Some((
            "residual-caster-half: HealCaster is unobservable on the single-entity cast (needs M25)",
            "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:399",
        ));
    }
    if any_payload(props, &|payload| matches!(payload, Payload::HealthLimiter(_))) {
        return Some((
            "residual-live-limiter: HealthLimiter is a per-minute live value, Rolled resolves to 0",
            "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:404",
        ));
    }
    None
}

/// Drives one template through the engine's own paths (cast, magic rounds
/// until expiry, declared item triggers) and reports whether anything
/// observable changed, plus the declared-residual reason when nothing did.
fn template_coverage(props: &'static EffectProperties) -> (usize, Option<(&'static str, &'static str)>) {
    let mut target = neutral_target();
    let mut dice = FixedDice::new(&[5, 0]);
    let mut state = MagicState::default();
    let mut observed = 0usize;
    let report = state.cast_effects(
        &[EffectEntry {
            props,
            settings: coverage_settings(),
        }],
        1,
        &mut target,
        &mut dice,
    );
    observed += report
        .applications
        .iter()
        .map(|application| application.changes.len())
        .sum::<usize>();
    observed += report.state_changes.len();
    for _ in 0..8 {
        if state.active().is_empty() {
            break;
        }
        let round = state.magic_round_with(1, &mut target, &mut dice);
        observed += round
            .applications
            .iter()
            .map(|application| application.changes.len())
            .sum::<usize>();
        observed += round.state_changes.len();
        if !round.expired.is_empty() {
            break;
        }
    }
    for payload in props.payload {
        if let Payload::On { trigger, .. } = payload {
            let mut bearer = neutral_target();
            let mut struck = neutral_target();
            let report = run_enchantment(props, *trigger, 10, &mut bearer, &mut struck, &mut dice);
            observed += report.bearer.len() + report.struck.len();
        }
    }
    if observed > 0 {
        (observed, None)
    } else {
        (0, Some(declared_residual(props).unwrap_or((
            "dead: no observable mutation on any engine path",
            "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:308",
        ))))
    }
}

fn block_a(out: &mut Out) {
    let rows: Vec<&'static EffectProperties> = templates().collect();
    let mut schools: Vec<(&'static str, usize, Vec<&'static str>)> = Vec::new();
    let mut seen_keys: Vec<&'static str> = Vec::new();
    let mut duplicate_keys = 0usize;
    let mut dead_rows = 0usize;

    for props in &rows {
        let school = school_label(props.magic_skill);
        let class = class_of(props);
        match schools.iter_mut().find(|(label, _, _)| *label == school) {
            Some((_, count, classes)) => {
                *count += 1;
                if !classes.contains(&class) {
                    classes.push(class);
                }
            }
            None => schools.push((school, 1, vec![class])),
        }
        if seen_keys.contains(&props.key) {
            duplicate_keys += 1;
        } else {
            seen_keys.push(props.key);
        }

        let (observed, residual) = template_coverage(props);
        let source = source_of(props.key);
        match residual {
            None => out.evidence(format!(
                "EFFECT {} school={} classic_key={} verdict=covered applied={} src={}",
                props.key, school, props.classic_key, observed, source
            )),
            Some((reason, spec)) => {
                if reason.starts_with("dead:") {
                    dead_rows += 1;
                }
                out.evidence(format!(
                    "EFFECT {} school={} classic_key={} verdict=NEGATIVE applied=0 reason={} src={} spec={spec}",
                    props.key, school, props.classic_key, reason, source
                ));
            }
        }
    }

    schools.sort_by(|left, right| left.0.cmp(right.0));
    let mut rows_total = 0usize;
    let mut classes_total = 0usize;
    for (school, count, classes) in &schools {
        rows_total += *count;
        classes_total += classes.len();
        out.evidence(format!(
            "SCHOOL {school} effects={} rows={count}",
            classes.len()
        ));
    }

    let measured = rows_total;
    let variants = rows_total.saturating_sub(classes_total);
    out.check_n(
        "A1",
        measured as i64,
        TEMPLATE_COUNT as i64,
        &format!(
            "registry rows={measured} declared={} classes={classes_total} variants={variants} schools={} unique_keys={} duplicates={duplicate_keys}",
            TEMPLATE_COUNT,
            schools.len(),
            seen_keys.len()
        ),
        SRC_TEMPLATE_COUNT,
    );
    out.check_n(
        "A2",
        classes_total as i64,
        143,
        "concrete effect classes = 153 source files - 10 abstract bases (contract §1)",
        "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:14",
    );
    out.check_n(
        "A3",
        variants as i64,
        22,
        "variant rows = ElementalResistance 5 + MageLight 5 + Pacify 4 + Poison 12 - 4 classes",
        "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:22",
    );
    out.check_n(
        "A4",
        duplicate_keys as i64,
        0,
        &format!("registry keys unique ({}/{TEMPLATE_COUNT})", seen_keys.len()),
        SRC_TEMPLATES,
    );
    out.check_n(
        "A5",
        rows.len() as i64,
        EFFECT_SOURCES.len() as i64,
        "every registry row has exactly one measured source citation",
        SRC_TEMPLATES,
    );
    out.check_n(
        "A6",
        dead_rows as i64,
        0,
        "per-template coverage: no registered template is dead on every engine path (NEGATIVE only for declared residual classes)",
        "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:293",
    );
    out.check_n(
        "A7",
        rows_total as i64,
        (143 + 22) as i64,
        "declared census reconciles: 165 rows = 143 concrete + 22 typed variants against 153 DFU files",
        "docs/sintergic/game/programming/m23-effects-contract-CR-59d7fd46.md:17",
    );
}

// ── block B · round durations ──────────────────────────────────────────────

/// One golden vector of the round-duration formula (spec §2.1).
struct RoundVector {
    id: &'static str,
    effect: &'static str,
    base: i32,
    plus: i32,
    per_level: i32,
    level: i32,
    want: i32,
}

fn round_row(out: &mut Out, vector: RoundVector) {
    let RoundVector { id, effect, base, plus, per_level, level, want } = vector;
    let got = rounds(&only_duration(base, plus, per_level), true, level);
    out.evidence(format!(
        "ROUND {effect} base={base} mod={plus} per_level={per_level} level={level} rounds={got} src={SRC_ROUNDS}"
    ));
    out.check_n(id, got as i64, want as i64, &format!("rounds({base},{plus},{per_level}) at level {level}"), SRC_ROUNDS);
}

fn block_b(out: &mut Out) {
    // Spec §2.1 B1-B5 (`m23-magic-spec-CR-5243af63.md:136-140`).
    round_row(out, RoundVector { id: "B1", effect: "Climbing", base: 1, plus: 1, per_level: 1, level: 1, want: 2 });
    round_row(out, RoundVector { id: "B2", effect: "Levitate", base: 1, plus: 1, per_level: 25, level: 1, want: 2 });
    round_row(out, RoundVector { id: "B3", effect: "Shield", base: 1, plus: 1, per_level: 1, level: 5, want: 6 });
    round_row(out, RoundVector { id: "B4", effect: "Pacify-Animal", base: 5, plus: 2, per_level: 3, level: 9, want: 11 });
    round_row(out, RoundVector { id: "B5", effect: "WaterWalking", base: 2, plus: 3, per_level: 2, level: 7, want: 11 });
    // B6: without `SupportDuration` the formula is 0 (`EntityEffect.cs:931-932`).
    let zero = rounds(&only_duration(9, 9, 1), false, 50);
    out.evidence(format!(
        "ROUND B6 base=9 mod=9 per_level=1 level=50 rounds={zero} src={SRC_ROUNDS}"
    ));
    out.check_n("B6", zero as i64, 0, "SupportDuration=false ⇒ 0 rounds", SRC_ROUNDS);
    // Per-level maximum of the vectors: level 100 with step 1.
    round_row(out, RoundVector { id: "B7", effect: "Cure-Disease", base: 1, plus: 1, per_level: 1, level: 100, want: 101 });
    // W41-3 fix: a hand-built `perLevel = 0` contributes no step instead of
    // dividing by zero (contract §5 item 5), and the floor still applies.
    round_row(out, RoundVector { id: "B8", effect: "Slowfall", base: 5, plus: 1, per_level: 0, level: 7, want: 6 });
    // The other half of the same guard: `chance_value` with `perLevel = 0`.
    let chance_zero = chance_value(&only_chance(6, 1, 0), 25);
    out.check_n(
        "B9",
        chance_zero as i64,
        6,
        "chance_value perLevel=0 keeps the base instead of dividing by zero",
        SRC_CHANCE,
    );
}

// ── block C · cost = formula ───────────────────────────────────────────────

/// One golden measurement of a classic cost formula.
struct CostRow<'a> {
    formula: &'a str,
    vector: &'a str,
    field: &'a str,
    got: i32,
    want: i32,
    derivation: &'a str,
    src: &'a str,
}

fn cost_row(out: &mut Out, row: CostRow<'_>) {
    let CostRow { formula, vector, field, got, want, derivation, src } = row;
    out.evidence(format!(
        "COST formula={formula} vector={vector} field={field} got={got} want={want} derivation={derivation} src={src}"
    ));
    out.check_n(
        &format!("{formula}-{vector}-{field}"),
        got as i64,
        want as i64,
        &format!("{formula} {field} of {vector}"),
        src,
    );
}

/// Route (a) totals: `(gold, spell_points)`.
fn total(entries: &[EffectEntry], target: TargetType, skill: i32) -> (i32, i32) {
    let cost = calculate_total_effect_costs(entries, target, false, &|_| skill);
    (cost.gold, cost.spell_points)
}

fn block_c(out: &mut Out) {
    // ── formula=C: `CalculateTotalEffectCosts` (spec §3.1, goldens D1-D12) ──
    let damage_unit = entry("Damage-Health", only_magnitude(1, 1, 1, 1, 1));
    let (gold, points) = effect_cost(&damage_unit, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D1",
            field: "gold",
            got: gold,
            want: 48,
            derivation: "20*1+28*1",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D1",
            field: "points",
            got: points,
            want: 7,
            derivation: "48*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    let (gold, points) = effect_cost(&damage_unit, 0);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D2",
            field: "gold",
            got: gold,
            want: 48,
            derivation: "20*1+28*1",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D2",
            field: "points",
            got: points,
            want: 13,
            derivation: "48*110/400",
            src: SRC_EFFECT_COST,
        },
    );

    let (gold, points) = effect_cost(&damage_unit, 100);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D3",
            field: "gold",
            got: gold,
            want: 48,
            derivation: "20*1+28*1",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D3",
            field: "points",
            got: points,
            want: 1,
            derivation: "48*10/400",
            src: SRC_EFFECT_COST,
        },
    );
    let (gold, points) = total(&[damage_unit], TargetType::ByTouch, 100);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D3",
            field: "total_gold",
            got: gold,
            want: 48,
            derivation: "48*1",
            src: SRC_TOTAL_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D3",
            field: "total_points",
            got: points,
            want: 5,
            derivation: "max(1,5)",
            src: SRC_TOTAL_COST,
        },
    );

    let levitate_unit = entry("Levitate", only_duration(1, 1, 1));
    let (gold, points) = effect_cost(&levitate_unit, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D4",
            field: "gold",
            got: gold,
            want: 160,
            derivation: "60*1+100*1",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D4",
            field: "points",
            got: points,
            want: 24,
            derivation: "160*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    let levitate_5 = entry("Levitate", only_duration(5, 3, 2));
    let (gold, points) = effect_cost(&levitate_5, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D5",
            field: "gold",
            got: gold,
            want: 400,
            derivation: "60*5+100*(3/2)",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D5",
            field: "points",
            got: points,
            want: 60,
            derivation: "400*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    let resistance = entry(
        "SpellResistance",
        EffectSettings {
            duration_base: 1,
            duration_plus: 1,
            duration_per_level: 1,
            chance_base: 1,
            chance_plus: 1,
            chance_per_level: 1,
            ..EffectSettings::classic_default()
        },
    );
    let (gold, points) = effect_cost(&resistance, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D6",
            field: "gold",
            got: gold,
            want: 240,
            derivation: "(20*1+100*1)+(20*1+100*1)",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D6",
            field: "points",
            got: points,
            want: 36,
            derivation: "240*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    let teleport = entry("Teleport-Effect", EffectSettings::classic_default());
    let (gold, points) = effect_cost(&teleport, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D7",
            field: "gold",
            got: gold,
            want: 320,
            derivation: "160+60*1+100*1",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D7",
            field: "points",
            got: points,
            want: 48,
            derivation: "320*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    let bundle = [damage_unit, levitate_unit];
    let (gold, points) = total(&bundle, TargetType::ByTouch, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D8",
            field: "gold",
            got: gold,
            want: 208,
            derivation: "48+160",
            src: SRC_TOTAL_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D8",
            field: "points",
            got: points,
            want: 31,
            derivation: "7+24",
            src: SRC_TOTAL_COST,
        },
    );
    let (gold, points) = total(&bundle, TargetType::SingleTargetAtRange, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D9",
            field: "gold",
            got: gold,
            want: 312,
            derivation: "208*1.5",
            src: SRC_RANGE_MULT,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D9",
            field: "points",
            got: points,
            want: 46,
            derivation: "(int)(31*1.5)",
            src: SRC_RANGE_MULT,
        },
    );
    let (gold, points) = total(&bundle, TargetType::AreaAroundCaster, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D10",
            field: "gold",
            got: gold,
            want: 416,
            derivation: "208*2.0",
            src: SRC_RANGE_MULT,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D10",
            field: "points",
            got: points,
            want: 62,
            derivation: "(int)(31*2.0)",
            src: SRC_RANGE_MULT,
        },
    );
    let (gold, points) = total(&bundle, TargetType::AreaAtRange, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D11",
            field: "gold",
            got: gold,
            want: 520,
            derivation: "208*2.5",
            src: SRC_RANGE_MULT,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "D11",
            field: "points",
            got: points,
            want: 77,
            derivation: "(int)(31*2.5)",
            src: SRC_RANGE_MULT,
        },
    );

    // The formula, not a table: 10× the magnitude averages cost 10× the
    // component (`start=10`, `increase=10`) instead of a fixed row.
    let damage_ten = entry("Damage-Health", only_magnitude(10, 10, 10, 10, 1));
    let (gold, points) = effect_cost(&damage_ten, 50);
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "SCALE-10X",
            field: "gold",
            got: gold,
            want: 480,
            derivation: "20*10+28*10",
            src: SRC_EFFECT_COST,
        },
    );
    cost_row(
        out,
        CostRow {
            formula: "C",
            vector: "SCALE-10X",
            field: "points",
            got: points,
            want: 72,
            derivation: "480*60/400",
            src: SRC_EFFECT_COST,
        },
    );

    // The explicit "formula, not a table" row: the same effect under three live
    // skills and one 10x settings change tracks the formula inputs.
    let skill_50 = effect_cost(&damage_unit, 50);
    let skill_0 = effect_cost(&damage_unit, 0);
    let skill_100 = effect_cost(&damage_unit, 100);
    let scaled = effect_cost(&damage_ten, 50);
    let derived = skill_50.0 == skill_0.0
        && skill_0.0 == skill_100.0
        && skill_50.1 == 7
        && skill_0.1 == 13
        && skill_100.1 == 1
        && scaled.0 == skill_50.0 * 10;
    out.check_bool(
        "C-NOT-A-TABLE",
        derived,
        true,
        "cost is recomputed from settings + live skill (skill sweep points 7/13/1, settings x10 => gold x10), never a lookup row",
        SRC_EFFECT_COST,
    );

    // ── formula=B: `CalculateCastingCost` (spec §3.2, goldens E1-E5) ────────
    let lock = slot(16, -1, [1, 1, 25], [6, 1, 10], [0; 5]);
    let got = calculate_casting_cost(&[lock], 0, true, &|_| 50);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "E1",
            field: "cost",
            got,
            want: 28,
            derivation: "47*60/100;28*2>>1",
            src: SRC_CASTING_COST,
        },
    );
    let got = calculate_casting_cost(&[lock], 4, true, &|_| 50);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "E2",
            field: "cost",
            got,
            want: 70,
            derivation: "28*5>>1",
            src: SRC_CASTING_COST,
        },
    );
    let levitate_slot = slot(14, -1, [2, 1, 1], [1, 1, 1], [0; 5]);
    let got = calculate_casting_cost(&[levitate_slot], 0, true, &|_| 50);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "E3",
            field: "cost",
            got,
            want: 33,
            derivation: "55*60/100",
            src: SRC_CASTING_COST,
        },
    );
    let tame = slot(33, 0, [5, 5, 1], [10, 10, 1], [0; 5]);
    let got = calculate_casting_cost(&[tame], 1, true, &|_| 50);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "E4",
            field: "cost",
            got,
            want: 360,
            derivation: "600*60/100;360*2>>1",
            src: SRC_CASTING_COST,
        },
    );
    let got = enchant_pt_cost(&[tame], 1);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "E5",
            field: "cost",
            got,
            want: 3600,
            derivation: "10*360",
            src: SRC_ENCHANT_PT,
        },
    );
    // Floor: a zero-coefficient row never costs less than 5 (`:2563-2564`).
    let wizard = slot(36, -1, [0, 0, 1], [0, 0, 1], [0, 0, 0, 0, 1]);
    let got = calculate_casting_cost(&[wizard], 0, true, &|_| 50);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "FLOOR",
            field: "cost",
            got,
            want: 5,
            derivation: "max(0,5)",
            src: SRC_CASTING_COST,
        },
    );
    // Skill dependence with `enchantingItem = false`: live skill 0 ⇒
    // `47*110/100 = 51`, so the cost is a function of the skill, not a table.
    let got = calculate_casting_cost(&[lock], 0, false, &|_| 0);
    cost_row(
        out,
        CostRow {
            formula: "B",
            vector: "SKILL-0",
            field: "cost",
            got,
            want: 51,
            derivation: "47*110/100;51*2>>1",
            src: SRC_CASTING_COST,
        },
    );
}

// ── block D · range / target ───────────────────────────────────────────────

fn block_d(out: &mut Out) {
    let base_gold = 208;
    let base_points = 31;
    let targets = [
        ("CasterOnly", TargetType::CasterOnly, 100),
        ("ByTouch", TargetType::ByTouch, 100),
        ("SingleTargetAtRange", TargetType::SingleTargetAtRange, 150),
        ("AreaAroundCaster", TargetType::AreaAroundCaster, 200),
        ("AreaAtRange", TargetType::AreaAtRange, 250),
    ];
    for (name, target, multiplier) in targets {
        let gold = apply_target_multiplier(base_gold, target);
        let points = apply_target_multiplier(base_points, target);
        out.evidence(format!(
            "RANGE {name} classic={} multiplier={multiplier} gold={gold} points={points} src={SRC_RANGE_MULT}",
            target.classic_index()
        ));
        let want_gold = base_gold * multiplier / 100;
        let want_points = (base_points as f32 * multiplier as f32 / 100.0) as i32;
        out.check_n(&format!("D-{name}"), gold as i64, want_gold as i64, "target gold multiplier", SRC_RANGE_MULT);
        out.check_n(&format!("D-{name}-points"), points as i64, want_points as i64, "target points multiplier", SRC_RANGE_MULT);
        out.check_bool(
            &format!("D-{name}-parse"),
            TargetType::from_classic_index(target.classic_index()) == Some(target),
            true,
            "classic rangeType roundtrip",
            SRC_RANGE_PARSE,
        );
    }
    out.check_bool(
        "D-unknown-range",
        TargetType::from_classic_index(5).is_none(),
        true,
        "rangeType 5 is refused (DFU throws, `EntityEffectBroker.cs:986-999`)",
        SRC_RANGE_PARSE,
    );
    out.check_bool(
        "D-default",
        TargetType::default() == TargetType::CasterOnly,
        true,
        "default target is CasterOnly (rangeType 0)",
        SRC_RANGE_MULT,
    );

    // Route (b) range modifiers `{2,2,3,4,5}` (`FormulaHelper.cs:2530,2562`).
    let lock = slot(16, -1, [1, 1, 25], [6, 1, 10], [0; 5]);
    let modifiers = [2, 2, 3, 4, 5];
    for (range_type, modifier) in modifiers.iter().enumerate() {
        let got = calculate_casting_cost(&[lock], range_type as u8, true, &|_| 50);
        let want = (28 * modifier) >> 1;
        out.evidence(format!(
            "RANGE-MOD classic={range_type} modifier={modifier} cost={got} src={SRC_CASTING_COST}"
        ));
        out.check_n(
            &format!("D-mod-{range_type}"),
            got as i64,
            want as i64,
            "classic rangeTypeModifiers",
            SRC_CASTING_COST,
        );
    }
}

// ── block E · spellmaker + persistence ─────────────────────────────────────

/// One slot of the classic record for an entry, or `None` when the settings do
/// not fit the record's `u8` fields (`ClassicEffectSettings` reads them back).
fn classic_slot(entry: &EffectEntry) -> Option<EffectSlot> {
    if entry.props.classic_key == 0 {
        return None;
    }
    let to_u8 = |value: i32| u8::try_from(value).ok();
    let (_, group, sub_group) = reverse_classic_key(entry.props.classic_key);
    Some(EffectSlot {
        effect_type: group as i8,
        sub_type: if sub_group == 255 { -1 } else { sub_group as i8 },
        duration: [
            to_u8(entry.settings.duration_base)?,
            to_u8(entry.settings.duration_plus)?,
            to_u8(entry.settings.duration_per_level)?,
        ],
        chance: [
            to_u8(entry.settings.chance_base)?,
            to_u8(entry.settings.chance_plus)?,
            to_u8(entry.settings.chance_per_level)?,
        ],
        magnitude: [
            to_u8(entry.settings.magnitude_base_min)?,
            to_u8(entry.settings.magnitude_base_max)?,
            to_u8(entry.settings.magnitude_plus_min)?,
            to_u8(entry.settings.magnitude_plus_max)?,
            to_u8(entry.settings.magnitude_per_level)?,
        ],
    })
}

/// Encodes the bundle into the **existing** 89-byte `SPELLS.STD` record layout
/// (`spells_std.rs`, spec §1.2) — no new format is invented: the same bytes
/// `parse_spells_std` reads back. `element` is not part of `SpellBundle`
/// (declared boundary) and is written as 4 (`Magic`, the retail dominant);
/// `index` is `0xFF` for a custom spell (no standard index).
fn encode_spell_record(bundle: &SpellBundle, cost: u16) -> Option<[u8; SPELL_RECORD_SIZE]> {
    let mut out = [0u8; SPELL_RECORD_SIZE];
    for i in 0..EFFECT_SLOT_COUNT {
        let base = EFFECT_TYPES_OFFSET + 2 * i;
        out[base] = 0xFF;
        out[base + 1] = 0xFF;
        for k in 0..3 {
            out[DURATION_OFFSET + 3 * i + k] = 1;
            out[CHANCE_OFFSET + 3 * i + k] = 1;
        }
        for k in 0..5 {
            out[MAGNITUDE_OFFSET + 5 * i + k] = 1;
        }
    }
    for (i, effect) in bundle.effects.iter().enumerate().take(EFFECT_SLOT_COUNT) {
        let slot = classic_slot(effect)?;
        let base = EFFECT_TYPES_OFFSET + 2 * i;
        out[base] = slot.effect_type as u8;
        out[base + 1] = slot.sub_type as u8;
        for k in 0..3 {
            out[DURATION_OFFSET + 3 * i + k] = slot.duration[k];
            out[CHANCE_OFFSET + 3 * i + k] = slot.chance[k];
        }
        for k in 0..5 {
            out[MAGNITUDE_OFFSET + 5 * i + k] = slot.magnitude[k];
        }
    }
    out[ELEMENT_OFFSET] = 4;
    out[RANGE_TYPE_OFFSET] = bundle.target.classic_index();
    out[COST_OFFSET..COST_OFFSET + 2].copy_from_slice(&cost.to_le_bytes());
    let name = bundle.name.as_bytes();
    let length = name.len().min(SPELL_NAME_BYTES - 1);
    out[SPELL_NAME_OFFSET..SPELL_NAME_OFFSET + length].copy_from_slice(&name[..length]);
    out[ICON_OFFSET] = bundle.icon;
    out[SPELL_INDEX_OFFSET] = 0xFF;
    Some(out)
}

/// Rebuilds the bundle from a parsed record through the classic view and the
/// engine's own `classic_effect_entry`.
fn decode_spell_record(record: &daggerfall_parser_rs::spells_std::SpellRecord) -> Option<SpellBundle> {
    let mut effects = Vec::new();
    for parsed in &record.effects {
        if !parsed.is_used() {
            continue;
        }
        let classic = slot(
            parsed.effect_type,
            parsed.sub_type,
            parsed.duration,
            parsed.chance,
            parsed.magnitude,
        );
        effects.push(classic_effect_entry(&classic)?);
    }
    Some(SpellBundle {
        name: record.spell_name.clone(),
        effects,
        target: TargetType::from_classic_index(record.range_type)?,
        minimum_casting_cost: false,
        icon: record.icon,
        standard_index: None,
    })
}

fn block_e(out: &mut Out) {
    let damage = entry("Damage-Health", only_magnitude(3, 6, 1, 4, 1));
    let levitate = entry("Levitate", only_duration(2, 1, 1));
    let resistance = entry(
        "SpellResistance",
        EffectSettings {
            duration_base: 2,
            duration_plus: 1,
            duration_per_level: 1,
            chance_base: 2,
            chance_plus: 1,
            chance_per_level: 1,
            ..EffectSettings::classic_default()
        },
    );

    let mut maker = SpellMaker::new(7, TargetType::ByTouch);
    maker.name = "GateComposedSpell".to_string();
    for (slot, effect) in [damage, levitate, resistance].into_iter().enumerate() {
        if !out.slot("E0-spellmaker", slot + 1, maker.add_effect(effect)) {
            return;
        }
    }

    let live = |_props: &EffectProperties| 50;
    let cost = maker.displayed_cost(&live);
    let mut destruction = 0;
    let mut thaumaturgy = 0;
    for effect in maker.entries() {
        let (gold, _) = effect_cost(effect, 50);
        match effect.props.magic_skill {
            MagicSkill::Destruction => destruction += gold,
            MagicSkill::Thaumaturgy => thaumaturgy += gold,
            MagicSkill::None
            | MagicSkill::Restoration
            | MagicSkill::Illusion
            | MagicSkill::Alteration
            | MagicSkill::Mysticism => {}
        }
    }
    out.evidence(format!(
        "SPELL name={} effects={} target=ByTouch gold={} points={} school_split=Destruction:{destruction},Thaumaturgy:{thaumaturgy} src={SRC_SPELLMAKER}",
        maker.name,
        maker.entries().len(),
        cost.gold,
        cost.spell_points
    ));
    out.check_n("E1", cost.gold as i64, 636, "composed gold (136 + 220 + 280)", SRC_TOTAL_COST);
    out.check_n("E2", cost.spell_points as i64, 95, "composed points (20 + 33 + 42)", SRC_TOTAL_COST);
    out.check_n(
        "E3",
        (destruction + thaumaturgy) as i64,
        636,
        &format!("school split sums to gold (Destruction {destruction} + Thaumaturgy {thaumaturgy})"),
        SRC_EFFECT_COST,
    );
    out.check_n("E4", maker.entries().len() as i64, 3, "three effects fit the spellmaker", SRC_SLOT_CAP);
    out.check(
        "E5",
        maker.name.clone(),
        "GateComposedSpell".to_string(),
        "composed spell name",
        SRC_SPELL_BUNDLE,
    );

    // Persistence through the crate's own byte encoding: encode into the
    // 89-byte record, parse it back with the real parser, resolve the entries
    // and re-encode — byte identity is the roundtrip.
    let bundle = SpellBundle {
        name: maker.name.clone(),
        effects: maker.entries().to_vec(),
        target: maker.target,
        minimum_casting_cost: false,
        icon: maker.icon,
        standard_index: None,
    };
    let Ok(cost_field) = u16::try_from(cost.gold) else {
        out.failures += 1;
        out.evidence(format!(
            "FAIL SPELLPERSIST got=gold-{} want=u16-record-field",
            cost.gold
        ));
        return;
    };
    let bytes = encode_spell_record(&bundle, cost_field);
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => {
            out.failures += 1;
            out.evidence("FAIL SPELLPERSIST got=encode-rejected want=encoded".to_string());
            return;
        }
    };
    let records = parse_spells_std(&bytes);
    let identical = records.len() == 1
        && decode_spell_record(&records[0]).is_some_and(|decoded| {
            let reencoded = encode_spell_record(&decoded, records[0].cost);
            let effects_equal = decoded.effects.len() == bundle.effects.len()
                && decoded
                    .effects
                    .iter()
                    .zip(bundle.effects.iter())
                    .all(|(left, right)| {
                        left.props.key == right.props.key && left.settings == right.settings
                    });
            effects_equal
                && decoded.name == bundle.name
                && decoded.icon == bundle.icon
                && decoded.target == bundle.target
                && reencoded.is_some_and(|reencoded| reencoded == bytes)
        });
    let digest = crate::m37_ui1_probe::sha256_hex(&bytes);
    out.evidence(format!(
        "SPELLPERSIST roundtrip={} digest={digest} bytes={} fields=name,icon,target,effects,cost src={SRC_RECORD_LAYOUT}",
        if identical { "identical" } else { "diverged" },
        bytes.len(),
    ));
    out.check_n("E6", records.len() as i64, 1, "the 89-byte record parses back as one spell", SRC_RECORD_LAYOUT);
    out.check_n("E7", records[0].cost as i64, 636, "record cost field carries the computed gold (spec §6 N-4: write-only)", SRC_RECORD_LAYOUT);
    out.check_bool("E8", identical, true, "composed spell survives encode → parse → re-encode byte-identically", SRC_RECORD_LAYOUT);
}

// ── block F · adversarial ──────────────────────────────────────────────────

fn block_f(out: &mut Out) {
    // F1 — an unknown effect key is refused, not silently mapped.
    let mut unresolved = Vec::new();
    let unknown_slots = [slot(60, 0, [1, 1, 1], [1, 1, 1], [1, 1, 1, 1, 1])];
    let entries = resolve_classic_effects(&unknown_slots, &mut unresolved);
    let refused = effect_by_key("No-Such-Effect").is_none()
        && effect_by_classic_key(0x7fff).is_none()
        && entries.is_empty()
        && unresolved == vec![classic_key(60, 0)];
    out.check_bool(
        "F1",
        refused,
        true,
        &format!("unknown key/classic key refused; unresolved={unresolved:?}"),
        SRC_RESOLVE,
    );

    // F2 — the 4th spellmaker slot is refused.
    let mut maker = SpellMaker::default();
    let effect = entry("Damage-Health", only_magnitude(1, 1, 1, 1, 1));
    for filled in 0..MAX_EFFECTS_PER_SPELL {
        if !out.slot("F2-fill", filled + 1, maker.add_effect(effect)) {
            return;
        }
    }
    let fourth = maker.add_effect(effect);
    out.check_bool(
        "F2",
        fourth == Err(SpellmakerError::NoFreeSlot),
        true,
        "4th effect refused with NoFreeSlot (RSC 1707)",
        SRC_SLOT_CAP,
    );

    // F3 — a magnitude beyond i32 saturates instead of wrapping negative.
    let mut dice = FixedDice::new(&[i32::MAX, i32::MAX]);
    let huge = EffectSettings {
        magnitude_base_min: i32::MAX,
        magnitude_base_max: i32::MAX,
        magnitude_plus_min: i32::MAX,
        magnitude_plus_max: i32::MAX,
        magnitude_per_level: 1,
        ..EffectSettings::classic_default()
    };
    let magnitude = roll_magnitude(&huge, true, 2, &mut dice);
    out.check_n(
        "F3",
        magnitude as i64,
        i32::MAX as i64,
        "magnitude overflow refused at i32 (saturating, never wrapping)",
        SRC_ROLL_MAGNITUDE,
    );

    // F4 — a cost component beyond i32 saturates instead of wrapping.
    let huge_cost = component_cost(make_effect_costs(i32::MAX, 0, 0), 2, 0, 1, 50);
    out.check_n(
        "F4",
        huge_cost as i64,
        i32::MAX as i64,
        "cost overflow refused at i32 (saturating, never wrapping)",
        SRC_COMPONENT_COST,
    );

    // F5 — a zero-duration buff must not silently become permanent.
    let mut target = flat_target();
    let mut state = MagicState::default();
    let mut dice = FixedDice::new(&[5, 0]);
    let report = state.cast_effects(
        &[entry("Fortify-Agility", only_duration(0, 0, 1))],
        1,
        &mut target,
        &mut dice,
    );
    let silent = state.active().is_empty()
        && report.state_changes.is_empty()
        && report.applications[0].changes.is_empty();
    out.evidence(format!(
        "ZERO-DURATION effect=Fortify-Agility rounds={} active={} state_changes={} src={SRC_PERMANENT}",
        rounds(&only_duration(0, 0, 1), true, 1),
        state.active().len(),
        report.state_changes.len(),
    ));
    out.check_bool(
        "F5",
        silent,
        true,
        "zero-duration buff applies nothing and is never kept live",
        SRC_PERMANENT,
    );

    // F6 — `SupportDuration = false` still keeps the curses live (the fix did
    // not turn the permanent templates into one-shot no-ops).
    let mut curse_target = flat_target();
    let mut curse_state = MagicState::default();
    let mut dice = FixedDice::new(&[5, 0]);
    let Some(curse) = out.require("F6-Vampirism-Curse", "registered", effect_by_key("Vampirism-Curse"))
    else {
        return;
    };
    let _ = curse_state.cast_effects(
        &[EffectEntry {
            props: curse,
            settings: only_duration(0, 0, 1),
        }],
        1,
        &mut curse_target,
        &mut dice,
    );
    out.check_bool(
        "F6",
        !curse_state.active().is_empty(),
        true,
        "a template without SupportDuration keeps its state live (curses)",
        SRC_PERMANENT,
    );

}

// ── main ───────────────────────────────────────────────────────────────────

pub fn run_sys_magic_001() -> i32 {
    let mut out = Out::new();
    out.lines.push(format!(
        "PROBE-SYS-MAGIC-001 - complete magic: per-template census, round durations, cost formulas, range/target, spellmaker persistence, adversarial refusals (M41/W41-3, {CR})"
    ));

    block_a(&mut out);
    block_b(&mut out);
    block_c(&mut out);
    block_d(&mut out);
    block_e(&mut out);
    block_f(&mut out);

    let pass = out.failures == 0;
    out.lines.push(format!(
        "VEREDICTO: {}",
        if pass { "PASS" } else { "FAIL" }
    ));
    let digest = crate::m37_ui1_probe::sha256_hex(out.lines.join("\n").as_bytes());
    out.lines.push(format!("DIGEST {digest}"));
    for line in &out.lines {
        println!("{line}");
    }
    i32::from(!pass)
}
