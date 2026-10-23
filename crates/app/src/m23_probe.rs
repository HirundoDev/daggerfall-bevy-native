//! M23 — anti-screen probes of **magic** (REQ-L99-056): `PROBE-MAGIC-001`
//! (parse + classic cost + casting), `PROBE-EFFECT-001` (effect model) and
//! `PROBE-SPELLMAKER-001` (creation).
//!
//! Contract: `docs/sintergic/game/programming/m23-magic-spec.md`
//! §9 (probe contract) + §10 (demo criterion). The goldens are transcribed
//! here with the erratum from "Corrections of this pass (2026-10-21)"
//! (#2 D3 → effect_points 1 / total 5; #3 E4 → 360; #4 E5 → 3600); the
//! formulas live in `daggerfall_behavior::magic` (spec §2/§3) and the parse
//! in `daggerfall_parser_rs::spells_std` (spec §1).
//!
//! `PROBE-MAGIC-001` — block A census §1.3 + goldens A1-A8 over the retail
//! `SPELLS.STD` (Mode A, 0 bytes in git); block B `§3.2` over the 89
//! records (`calc_cost_ok=89/89` + E1..E5); block C §3.1 over 12 synthetic
//! bundles D1-D12; block D demo casting (touch/range/area with points
//! subtracted = A of the bundle, rounds §2 and expiration §10.2).
//!
//! `PROBE-EFFECT-001` — block A goldens B1-B6 + C1-C6 (C6 without clamp);
//! block B runtime registration over the whole registry (`templates_registered`
//! = [`TEMPLATE_COUNT`] = 165 keys for the 153 DFU templates — 143 concrete +
//! 10 abstract bases — 0 collisions, 4 variant families / 22 typed variants);
//! block C instance isolation; block D the `(classicKey → effectKey)` map
//! (coverage + digest ×2); block E the **applied** layer: health/fatigue/spell
//! points, fortify + expiry, elemental/spell resistance, the three M19 movement
//! flags, cure/teleport, the failed-chance negative control and unknown classic
//! keys reported;
//! block F the **per-template evidence** of the 165 registered
//! keys (contract §5): the paths each one ran — cast/state/round/trigger —,
//! `applied > 0` on the neutral target, the duration round / permanent horizon
//! checks and the revert of every state field it owns on expiry (`F-<key>`,
//! one row per key); plus the **per-payload evidence** (§5.4-§5.10): every
//! payload leaf driven alone through the engine's own leaf entry points
//! (`MagicState::cast_leaf_effect`, `run_enchantment_leaf`) and judged
//! `observed == expected` against DFU-derived wants (`F-p-<key>-<i>`), the
//! entity-scope table of the item-trigger templates (`F-e-<key>`), a
//! negative-control pair with a dead-injection test (`F-neg-*`), the
//! entity-inversion antibody (`F-mut-entity-inversion`) and the frozen
//! per-payload census (`F-payload-census`).
//!
//! `PROBE-SPELLMAKER-001` — block A displayed vs. charged cost over 20
//! synthetic bundles (`mismatch=0`); block B limits (4th rejected, floor 5,
//! `minimumCastingCost`); block C purchase price
//! (46→184 / 46 festival→92 / 0 festival→1); block D animation (release
//! frame 5 = 0.20 s, 7 frames = 0.28 s).
//!
//! **Outputs (§9):** `EXIT=0` ⟺ all checks OK · `EXIT=1` on any FAIL
//! (row `FAIL <id> got=<v> want=<v>`) · `EXIT=2` if ARENA2 is not
//! accessible (`PROBE-MAGIC-001`, blocks A/B = census). EFFECT/SPELLMAKER are
//! 100% synthetic (spec §6 N-1: only the engine and the 17 reference
//! effects; 0 retail data ⇒ never EXIT=2).

use std::collections::{BTreeMap, VecDeque};

use daggerfall_behavior::magic::{
    apply_free_action_patch, calculate_total_effect_costs, cast_full_seconds,
    cast_release_seconds, casting_cost_enchanting, chance_value, classic_effect_entry,
    component_cost, diff_observations, effect_by_classic_key, effect_by_key, effect_cost,
    enchant_pt_cost, make_effect_costs, purchase_price, resolve_classic_effects, roll_chance,
    rounds, run_enchantment, run_enchantment_leaf, templates, Amount, Attribute, CastReport,
    ChanceFunction, ChanceState, ClassicEffect, Cure, EffectChange, EffectDice, EffectEntry,
    EffectProperties, EffectSettings, EffectTarget, Element, Flag, Lifetime, MagicSkill,
    MagicState, Payload, Request, Resource, SocialGroup, SpellMaker, SpellmakerError, TargetType,
    Trigger, ATTRIBUTE_COUNT, CAST_FRAME_INDICES, CAST_RELEASE_FRAME, EFFECT_SKILL_COUNT,
    FATIGUE_MULTIPLIER, PERMANENT_ROUNDS, SETTINGS_TYPES, TEMPLATE_COUNT,
};
use daggerfall_parser_rs::spells_std::{census, parse_spells_std, SPELL_RECORD_SIZE};

// ── evidence rows (M19/M20/M21/M22 pattern) ──────────────────────────────

/// Row printed by the probe: `[OK] <id> - <detail>` or
/// `FAIL <id> got=<v> want=<v>` (verbatim from spec §9).
#[derive(Debug, Clone)]
struct Row {
    id: String,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want`.
fn row(id: impl Into<String>, got: String, want: String, detail: impl Into<String>) -> Row {
    Row {
        id: id.into(),
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            // `got` and `want` travel on green too: an evidence row nobody
            // can audit without re-running is not evidence (QA F2.2 MINOR-4,
            // extended to the per-payload rows of contract §5.4).
            println!("[OK] {} - {} · got={} want={}", row.id, row.detail, row.got, row.want);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

/// FNV-1a 64 (same scheme as the M18/M20 and M22 demo digests).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ── synthetic helpers (blocks C/D, EFFECT and SPELLMAKER) ────────────────────

/// Duration settings `(base, plus, perLevel)`; the rest 0.
fn dur(base: i32, plus: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        duration_base: base,
        duration_plus: plus,
        duration_per_level: per_level,
        ..EffectSettings::default()
    }
}

/// Magnitude settings `(min, max, plusMin, plusMax, perLevel)` on top of the
/// classic defaults.
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

/// Entry `(reference template, settings)`.
fn entry(key: &str, settings: EffectSettings) -> EffectEntry {
    EffectEntry {
        props: effect_by_key(key).expect("missing reference template"),
        settings,
    }
}

/// Parser slot → engine classic slot (same fields, different types).
fn classic(slot: &daggerfall_parser_rs::spells_std::EffectSlot) -> ClassicEffect {
    ClassicEffect {
        effect_type: slot.effect_type,
        sub_type: slot.sub_type,
        duration: slot.duration,
        chance: slot.chance,
        magnitude: slot.magnitude,
    }
}

// ── PROBE-MAGIC-001 · block C · §3.1 over 12 synthetic bundles ──────────────

/// The 12 D1-D12 checks of block C (spec §9: exact `(gold, points)`;
/// D3 `effect_points=1` and total `5`; D9 `46`, D10 `77`). Pure: 0 I/O.
fn block_c_rows() -> Vec<Row> {
    let mut rows = Vec::with_capacity(12);

    // D1-D3: Damage-Health magnitude 1-1 + 1-1/1 (CostA 20, CostB 28).
    let damage = entry("Damage-Health", mag(1, 1, 1, 1, 1));
    let (gold, points) = effect_cost(&damage, 50);
    rows.push(row(
        "D1",
        format!("gold={gold} points={points}"),
        "gold=48 points=7".to_string(),
        "20·1 + 28·(1/1) = 48; 48·60/400 = 7 (skill 50)",
    ));
    let (gold, points) = effect_cost(&damage, 0);
    let total = calculate_total_effect_costs(&[damage], TargetType::ByTouch, false, &|_| 0);
    rows.push(row(
        "D2",
        format!("gold={gold} points={points} total={}", total.spell_points),
        "gold=48 points=13 total=13".to_string(),
        "48·110/400 = 13 (skill 0)",
    ));
    let (gold, points) = effect_cost(&damage, 100);
    let total = calculate_total_effect_costs(&[damage], TargetType::ByTouch, false, &|_| 100);
    rows.push(row(
        "D3",
        format!("gold={gold} effect_points={points} total={}", total.spell_points),
        "gold=48 effect_points=1 total=5".to_string(),
        "48·10/400 = 1; total floor 5 (erratum §12 #2)",
    ));

    // D4-D5: Levitate duration (CostA 60, CostB 100).
    let levitate = entry("Levitate", dur(1, 1, 1));
    let (gold, points) = effect_cost(&levitate, 50);
    rows.push(row(
        "D4",
        format!("gold={gold} points={points}"),
        "gold=160 points=24".to_string(),
        "60·1 + 100·(1/1) = 160 (Levitate.cs:37)",
    ));
    let levitate532 = entry("Levitate", dur(5, 3, 2));
    let (gold, points) = effect_cost(&levitate532, 50);
    rows.push(row(
        "D5",
        format!("gold={gold} points={points}"),
        "gold=400 points=60".to_string(),
        "60·5 + 100·(3/2 = 1) = 400",
    ));

    // D6: SpellResistance duration + chance.
    let sr_settings = EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        chance_base: 1,
        chance_plus: 1,
        chance_per_level: 1,
        ..EffectSettings::default()
    };
    let sr = entry("SpellResistance", sr_settings);
    let (gold, points) = effect_cost(&sr, 50);
    rows.push(row(
        "D6",
        format!("gold={gold} points={points}"),
        "gold=240 points=36".to_string(),
        "(20·1+100·1) + (20·1+100·1) = 240",
    ));

    // D7: Teleport without components ⇒ fudgeGold (60,100,160).
    let teleport = entry("Teleport-Effect", EffectSettings::classic_default());
    let (gold, points) = effect_cost(&teleport, 50);
    rows.push(row(
        "D7",
        format!("gold={gold} points={points}"),
        "gold=320 points=48".to_string(),
        "160 + 60·1 + 100·1 = 320 (without components)",
    ));

    // D8-D10: Damage + Levitate with the target multiplier.
    let two_entries = [damage, levitate];
    for (id, target, want) in [
        ("D8", TargetType::ByTouch, (208, 31)),
        (
            "D9",
            TargetType::SingleTargetAtRange,
            (312, 46), // (int)(31·1.5f) = 46
        ),
        ("D10", TargetType::AreaAtRange, (520, 77)), // (int)(77.5) = 77
    ] {
        let cost = calculate_total_effect_costs(&two_entries, target, false, &|_| 50);
        rows.push(row(
            id,
            format!("gold={} points={}", cost.gold, cost.spell_points),
            format!("gold={} points={}", want.0, want.1),
            format!("48+160 with target {target:?}"),
        ));
    }

    // D11: gold effect 8 ⇒ 1 point (per-effect formula, no floor).
    // Cost fixture only: never registered, so it declares no payload.
    const GOLD8: EffectProperties = EffectProperties {
        key: "D11-Gold-8",
        payload: &[],
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
    let gold8 = EffectEntry {
        props: &GOLD8,
        settings: dur(1, 1, 1),
    };
    let (gold, points) = effect_cost(&gold8, 50);
    rows.push(row(
        "D11",
        format!("gold={gold} points={points}"),
        "gold=8 points=1".to_string(),
        "8·60/400 = 480/400 = 1",
    ));

    // D12: average magnitude with integer division.
    let start = 4;
    let increase = 2;
    let component = component_cost(make_effect_costs(20, 28, 0), start, increase, 1, 50);
    rows.push(row(
        "D12",
        format!("start={start} increase={increase} component={component}"),
        "start=4 increase=2 component=136".to_string(),
        "(6+3)/2 = 4, (4+1)/2 = 2; 20·4 + 28·2 = 136",
    ));

    rows
}

// ── PROBE-MAGIC-001 · block D · demo casting (§9 D, §10.1-§10.2) ───────

/// Synthetic bundle D8 (instant Damage + Levitate 2 rounds @ level 1).
/// `pub(crate)` for the demo's M23 block (`m18_demo`, 0 duplicated logic).
/// Seed of the demo's strike block (the M23 sibling of `M22_SEED`): fixed so
/// the resolved attack and its callbacks are reproducible.
pub(crate) const M23_SEED: u64 = 23_261_022;

pub(crate) fn demo_entries() -> [EffectEntry; 2] {
    [
        entry("Damage-Health", mag(1, 1, 1, 1, 1)),
        entry("Levitate", dur(1, 1, 1)),
    ]
}

/// Canonical trace of the demo run (3 targets: cost, pool, rounds,
/// expiration) — block D digests it ×2 and requires equality.
fn demo_trace() -> String {
    let mut trace = String::new();
    for (index, target) in [
        TargetType::ByTouch,
        TargetType::SingleTargetAtRange,
        TargetType::AreaAtRange,
    ]
    .iter()
    .enumerate()
    {
        let entries = demo_entries();
        let cost = calculate_total_effect_costs(&entries, *target, false, &|_| 50);
        let pool_before = 200;
        let pool_after = pool_before - cost.spell_points;
        let mut state = MagicState::default();
        let starts = state.cast(&entries, 1);
        let expired_first = state.magic_round();
        let expired_second = state.magic_round();
        let rounds_end = state.rounds_left("Levitate");
        trace.push_str(&format!(
            "{index}|{target:?}|{}|{}|{pool_before}->{pool_after}|{starts:?}|{expired_first:?}|{expired_second:?}|{rounds_end};",
            cost.gold, cost.spell_points
        ));
    }
    trace
}

/// Block D: 3 casts (touch/range/area) + digest ×2 (spec §9 D). Pure.
fn block_d_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, (target, want)) in [
        (TargetType::ByTouch, (208, 31)),
        (TargetType::SingleTargetAtRange, (312, 46)),
        (TargetType::AreaAtRange, (520, 77)),
    ]
    .iter()
    .enumerate()
    {
        let entries = demo_entries();
        let cost = calculate_total_effect_costs(&entries, *target, false, &|_| 50);
        let pool_before = 200;
        let pool_after = pool_before - cost.spell_points;
        let mut state = MagicState::default();
        let starts = state.cast(&entries, 1);
        let expired_first = state.magic_round();
        let _ = state.magic_round();
        let rounds_end = state.rounds_left("Levitate");
        rows.push(row(
            match index {
                0 => "D1-touch",
                1 => "D2-range",
                _ => "D3-area",
            },
            format!(
                "gold={} points={} pool {pool_before}->{pool_after} starts={starts:?} exp1={expired_first:?} end={rounds_end}",
                cost.gold, cost.spell_points
            ),
            format!(
                "gold={} points={} pool {pool_before}->{} starts=[0, 2] exp1=[] end=0",
                want.0,
                want.1,
                pool_before - want.1
            ),
            format!(
                "points subtracted = A of the bundle · rounds §2 ({target:?}) · expires on round N (§10.2)"
            ),
        ));
    }
    // Digest ×2 of the complete canonical trace (independent run).
    let first = fnv1a64(demo_trace().as_bytes());
    let second = fnv1a64(demo_trace().as_bytes());
    rows.push(row(
        "D-digest",
        format!("digest={first}"),
        format!("digest={second}"),
        "demo trace digested ×2, identical",
    ));
    rows
}

// ── PROBE-MAGIC-001 · block A · census §1.3 + goldens A1-A8 ─────────────────

/// Block A: retail census (spec §1.3) + goldens A1-A8 (spec §1.4).
fn block_a_rows(bytes: &[u8]) -> Vec<Row> {
    let records = parse_spells_std(bytes);
    let censo = census(&records);
    let mut rows = Vec::with_capacity(19);

    rows.push(row(
        "A1",
        format!("bytes={} records={}", bytes.len(), records.len()),
        "bytes=7921 records=89".to_string(),
        "7921/89 exact, condition `position + 89 <= length`",
    ));
    let rec0 = &records[0];
    rows.push(row(
        "A2",
        format!(
            "slot0=({},{}) slot1=({},{}) slot2=({},{})",
            rec0.effects[0].effect_type,
            rec0.effects[0].sub_type,
            rec0.effects[1].effect_type,
            rec0.effects[1].sub_type,
            rec0.effects[2].effect_type,
            rec0.effects[2].sub_type
        ),
        "slot0=(16,-1) slot1=(-1,-1) slot2=(-1,-1)".to_string(),
        "bytes 10 ff ff ff ff ff; 0xFF → −1",
    ));
    rows.push(row(
        "A3",
        format!(
            "element={} range={} cost={}",
            rec0.element, rec0.range_type, rec0.cost
        ),
        "element=4 range=0 cost=0".to_string(),
        "offsets §1.2; cost 0x0000 LE",
    ));
    rows.push(row(
        "A4",
        format!(
            "dur0={:?} dur1={:?} dur2={:?}",
            rec0.effects[0].duration, rec0.effects[1].duration, rec0.effects[2].duration
        ),
        "dur0=[1, 1, 25] dur1=[1, 1, 1] dur2=[1, 1, 1]".to_string(),
        "bytes 14-22",
    ));
    rows.push(row(
        "A5",
        format!(
            "name={:?} icon={} index={}",
            rec0.spell_name, rec0.icon, rec0.index
        ),
        "name=\"Fenrik's Door Jam\" icon=1 index=1".to_string(),
        "bytes 47-51, 72, 73",
    ));
    let last = records.last().expect("89 records");
    rows.push(row(
        "A6",
        format!(
            "last_start={} name_off={} name={:?}",
            censo.last_start,
            censo.last_name_offset,
            last.spell_name
        ),
        "last_start=7832 name_off=7879 name=\"Tame\"".to_string(),
        "88·89; 7832 + 47",
    ));
    rows.push(row(
        "A7",
        format!(
            "element={} range={} cost={} index={}",
            last.element, last.range_type, last.cost, last.index
        ),
        "element=4 range=1 cost=2950 index=99".to_string(),
        "bytes 04 01 86 0b; 0x0B86 = 2950",
    ));
    rows.push(row(
        "A8",
        format!("stride={SPELL_RECORD_SIZE}"),
        "stride=89".to_string(),
        "6+1+1+2+4+9+9+15+25+1+1+15 = 89 (type==0xFF never fires, §1.2)",
    ));

    // Census §1.3 (anti-false (ii): records != 89 ⇒ A fails).
    rows.push(row(
        "C-internal",
        format!("internal={}", censo.internal),
        "internal=12".to_string(),
        "names prefixed with `!`",
    ));
    rows.push(row(
        "C-zero-cost",
        format!("zero_cost={}", censo.zero_cost),
        "zero_cost=8".to_string(),
        "records with cost = 0",
    ));
    rows.push(row(
        "C-max-cost",
        format!("max_cost={}", censo.max_cost),
        "max_cost=2950".to_string(),
        "maximum cost in the file",
    ));
    rows.push(row(
        "C-unique-index",
        format!("unique_index={}", censo.unique_indexes),
        "unique_index=88".to_string(),
        "89 records ⇒ 1 duplicate",
    ));
    rows.push(row(
        "C-offered",
        format!("offered={}", censo.offered),
        "offered=76".to_string(),
        "without `!` and with the duplicate discarded (broker :747-752)",
    ));
    let dup = censo.duplicates.first();
    rows.push(row(
        "C-duplicate",
        format!(
            "index={} first_cost={} second_cost={}",
            dup.map_or(0, |d| d.index),
            dup.map_or(0, |d| d.first_cost),
            dup.map_or(0, |d| d.second_cost)
        ),
        "index=58 first_cost=550 second_cost=550".to_string(),
        "the only duplicate: index 58, cost 550 on both",
    ));
    rows.push(row(
        "C-slots",
        format!(
            "one={} two={} three={}",
            censo.used_one, censo.used_two, censo.used_three
        ),
        "one=80 two=8 three=1".to_string(),
        "effect slots used per record",
    ));
    rows.push(row(
        "C-elements",
        format!(
            "Fire={} Cold={} Poison={} Shock={} Magic={}",
            censo.element_counts[0],
            censo.element_counts[1],
            censo.element_counts[2],
            censo.element_counts[3],
            censo.element_counts[4]
        ),
        "Fire=5 Cold=4 Poison=1 Shock=2 Magic=77".to_string(),
        "element 0..4 (EntityEffectBroker.cs:1010-1027)",
    ));
    rows.push(row(
        "C-ranges",
        format!(
            "0={} 1={} 2={} 3={} 4={}",
            censo.range_counts[0],
            censo.range_counts[1],
            censo.range_counts[2],
            censo.range_counts[3],
            censo.range_counts[4]
        ),
        "0=42 1=31 2=11 3=3 4=2".to_string(),
        "rangeType 0..4 (EntityEffectBroker.cs:986-999)",
    ));
    rows.push(row(
        "C-out-of-range",
        format!(
            "elements={} ranges={}",
            censo.element_out_of_range, censo.range_out_of_range
        ),
        "elements=0 ranges=0".to_string(),
        "anti-false (iii): a mishandled 0xFF ⇒ element > 4",
    ));

    rows
}

// ── PROBE-MAGIC-001 · block B · §3.2 over the 89 records ────────────────

/// Replica of the arithmetic precondition of `getCostFromSettings`
/// (`FormulaHelper.cs:2566-2632`): C# does not guard the 0 divisor and throws
/// `DivideByZeroException`. Measured 2026-10-21 on the retail data: the only
/// reachable slot is 1 of record 71 `!Indulcet` (index 79,
/// settingsType 6, `magnitudePerLevel` 0), only reachable via
/// `GetSpellEnchantPtCost(79)` — no live callers in DFU.
fn dfu_zero_divisor(settings_type: u8, effect: &ClassicEffect) -> Option<&'static str> {
    let duration_zero = effect.duration[2] == 0;
    let chance_zero = effect.chance[2] == 0;
    let magnitude_zero = effect.magnitude[4] == 0;
    match settings_type {
        1 => {
            if duration_zero {
                Some("durationPerLevel")
            } else if chance_zero {
                Some("chancePerLevel")
            } else {
                None
            }
        }
        2 | 6 => {
            if duration_zero {
                Some("durationPerLevel")
            } else if magnitude_zero {
                Some("magnitudePerLevel")
            } else {
                None
            }
        }
        3 => duration_zero.then_some("durationPerLevel"),
        4 => chance_zero.then_some("chancePerLevel"),
        5 => magnitude_zero.then_some("magnitudePerLevel"),
        _ => None,
    }
}

/// Block B: `CalculateCastingCost` (route b) over the 89 real records +
/// goldens E1-E5 (spec §3.3, erratum #3/#4) + R-E (`spell.cost` inert) +
/// N-5 (Free Action patch over the retail data). Erratum measured vs. spec §9:
/// record 71 `!Indulcet` has a 0 divisor and C# would throw the same
/// exception ⇒ `calc_cost_ok=88/89` + 1 documented skip, never a false PASS.
fn block_b_rows(records: &[daggerfall_parser_rs::spells_std::SpellRecord]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(10);

    // Computation over the computable records: 0 panic and floor ≥ 5 on all.
    let mut computed = 0usize;
    let mut zero_divisors: Vec<String> = Vec::new();
    for (position, record) in records.iter().enumerate() {
        let effects: Vec<ClassicEffect> = record.effects.iter().map(classic).collect();
        let mut divisor_zero: Option<String> = None;
        for (slot_index, effect) in effects.iter().enumerate() {
            if effect.effect_type == -1 {
                continue;
            }
            if let Some(settings_type) =
                SETTINGS_TYPES.get(usize::from(effect.effect_type as u8)).copied()
            {
                if let Some(field) = dfu_zero_divisor(settings_type, effect) {
                    divisor_zero = Some(format!(
                        "record={position} name={:?} index={} slot={slot_index} settingsType={settings_type} {field}=0",
                        record.spell_name, record.index
                    ));
                    break;
                }
            }
        }
        if let Some(evidence) = divisor_zero {
            zero_divisors.push(evidence);
            continue;
        }
        let _ = casting_cost_enchanting(&effects, record.range_type);
        computed += 1;
    }
    rows.push(row(
        "B-calc",
        format!(
            "calc_cost_ok={computed}/{} upstream_skip={}",
            records.len(),
            zero_divisors.len()
        ),
        "calc_cost_ok=88/89 upstream_skip=1".to_string(),
        "§3.2 over the computable records (erratum §9: 1 throw equivalent to C#)",
    ));
    rows.push(row(
        "B-upstream-zero-divisor",
        zero_divisors.join("; "),
        "record=71 name=\"!Indulcet\" index=79 slot=1 settingsType=6 magnitudePerLevel=0"
            .to_string(),
        "only 0 divisor in the retail data: equivalent DivideByZeroException (measured 2026-10-21)",
    ));

    // E1: record 0 (index 1, Lock) → 28. Erratum (measured 2026-10-21): the
    // spec §3.3 derived E1 with chance (1,1,1); the real record carries
    // chance (6,1,10): 5·1 + (1/25)·25 + 7·6 + (1/10)·30 = 47 ⇒ 47·60/100 = 28.
    let rec0 = &records[0];
    let rec0_effects: Vec<ClassicEffect> = rec0.effects.iter().map(classic).collect();
    let e1 = casting_cost_enchanting(&rec0_effects, rec0.range_type);
    rows.push(row(
        "E1",
        format!("cost={e1}"),
        "cost=28".to_string(),
        "47·60/100 = 28; 28·2>>1 = 28 (row 20 Lock, real chance (6,1,10))",
    ));

    // E2: same record with rangeType 4 → 70 (28·5>>1; was 62, carried over).
    let e2 = casting_cost_enchanting(&rec0_effects, 4);
    rows.push(row(
        "E2",
        format!("cost={e2}"),
        "cost=70".to_string(),
        "28·5 >> 1 = 140 >> 1",
    ));

    // E3: the retail record named "Levitate" (position 3) → 33.
    // Erratum: real dur (2,1,1), not (1,1,1): 15·2 + (1/1)·25 = 55 ⇒ 33.
    match records.iter().find(|record| record.spell_name == "Levitate") {
        Some(levitate_record) => {
            let effects: Vec<ClassicEffect> = levitate_record.effects.iter().map(classic).collect();
            let e3 = casting_cost_enchanting(&effects, levitate_record.range_type);
            rows.push(row(
                "E3",
                format!("cost={e3} range={}", levitate_record.range_type),
                "cost=33 range=0".to_string(),
                "55·60/100 = 33 (row 18 Levitate, real dur (2,1,1))",
            ));
        }
        None => {
            rows.push(row(
                "E3",
                "Levitate record absent".to_string(),
                "cost=24 range=0".to_string(),
                "the retail data must contain the Levitate spell",
            ));
        }
    }

    // E4-E5: record 88 (Tame, Calm) → 360 and ×10 = 3600 (erratum #3/#4).
    let last = records.last().expect("89 records");
    let last_effects: Vec<ClassicEffect> = last.effects.iter().map(classic).collect();
    let e4 = casting_cost_enchanting(&last_effects, last.range_type);
    rows.push(row(
        "E4",
        format!("cost={e4}"),
        "cost=360".to_string(),
        "600·60/100 = 360; cost·2>>1 = 360 (was 960, erratum #3)",
    ));
    let e5 = enchant_pt_cost(&last_effects, last.range_type);
    rows.push(row(
        "E5",
        format!("enchant={e5}"),
        "enchant=3600".to_string(),
        "10 × 360 (was 9600, erratum #4)",
    ));

    // R-E (§6 N-4 / R-E): the record's `cost` field does not alter §3.1/§3.2.
    let mut mutated = records[0].clone();
    mutated.cost = 2950;
    let mutated_effects: Vec<ClassicEffect> = mutated.effects.iter().map(classic).collect();
    let mutated_cost = casting_cost_enchanting(&mutated_effects, mutated.range_type);
    rows.push(row(
        "R-E-cost-inert",
        format!("cost0={e1} cost2950={mutated_cost}"),
        "cost0=28 cost2950=28".to_string(),
        "`spell.cost` does not take part in route (b) (§6 N-4)",
    ));

    // N-5: the retail record with `index 10` (position 9) stores (3,2) in
    // slot 0; the patch corrects it to Free Action (26,−1).
    match records.iter().find(|record| record.index == 10) {
        Some(free_action_record) => {
            let mut free_action = classic(&free_action_record.effects[0]);
            let (raw_type, raw_sub) = (free_action.effect_type, free_action.sub_type);
            apply_free_action_patch(10, 0, &mut free_action);
            let patched = classic_effect_entry(&free_action)
                .map(|effect_entry| effect_entry.props.key)
                .unwrap_or("(no template)");
            rows.push(row(
                "N5-free-action",
                format!(
                    "raw=({raw_type},{raw_sub}) patched=({},{}) template={patched}",
                    free_action.effect_type, free_action.sub_type
                ),
                "raw=(3,2) patched=(26,-1) template=FreeAction".to_string(),
                "index 10 slot 0 (3,2) → (26,−1) (`EntityEffectBroker.cs:892-899`)",
            ));
        }
        None => {
            rows.push(row(
                "N5-free-action",
                "record index 10 absent".to_string(),
                "raw=(3,2) patched=(26,-1) template=FreeAction".to_string(),
                "the retail data must contain the record with index 10",
            ));
        }
    }

    rows
}

// ── PROBE-EFFECT-001 · blocks A-D ───────────────────────────────────────────

/// Block A: goldens B1-B6 (rounds) and C1-C6 (chance) — spec §2.1.
fn effect_rows_a() -> Vec<Row> {
    let mut rows = Vec::with_capacity(12);
    for (id, (base, plus, per_level, level), want) in [
        ("B1", (1, 1, 1, 1), 2),
        ("B2", (1, 1, 25, 1), 2),
        ("B3", (1, 1, 1, 5), 6),
        ("B4", (5, 2, 3, 9), 11),
        ("B5", (2, 3, 2, 7), 11),
    ] {
        let settings = dur(base, plus, per_level);
        let got = rounds(&settings, true, level);
        rows.push(row(
            id,
            format!("rounds={got}"),
            format!("rounds={want}"),
            format!("({base},{plus},{per_level},{level}) ⇒ {want}"),
        ));
    }
    rows.push(row(
        "B6",
        format!(
            "rounds={} rounds_level50={}",
            rounds(&dur(9, 9, 1), false, 1),
            rounds(&dur(9, 9, 1), false, 50)
        ),
        "rounds=0 rounds_level50=0".to_string(),
        "SupportDuration=false ⇒ 0 (else branch)",
    ));

    // C1-C4: `ChanceValue` = base + plus·⌊level/chancePerLevel⌋.
    for (id, (base, plus, per_level, level), want) in [
        ("C1", (6, 1, 10, 5), 6),
        ("C2", (6, 1, 10, 25), 8),
        ("C3", (1, 1, 1, 7), 8),
        ("C4", (10, 5, 1, 3), 25),
    ] {
        let settings = EffectSettings {
            chance_base: base,
            chance_plus: plus,
            chance_per_level: per_level,
            ..EffectSettings::default()
        };
        let got = chance_value(&settings, level);
        rows.push(row(
            id,
            format!("chance={got}"),
            format!("chance={want}"),
            format!("({base},{plus},{per_level},{level}) ⇒ {want} %"),
        ));
    }

    // C5: value 0 never succeeds; C6: 100/150 always, without clamp.
    let zero = EffectSettings {
        chance_per_level: 1,
        ..EffectSettings::default()
    };
    let c5_value = chance_value(&zero, 50);
    let c5_rolls = (0..=99).all(|roll| !roll_chance(true, &zero, 50, roll));
    rows.push(row(
        "C5",
        format!("value={c5_value} rolls_0_99_all_false={c5_rolls}"),
        "value=0 rolls_0_99_all_false=true".to_string(),
        "roll 0..99 never < 0",
    ));
    let c100 = EffectSettings {
        chance_base: 50,
        chance_plus: 50,
        chance_per_level: 1,
        ..EffectSettings::default()
    };
    let c150 = EffectSettings {
        chance_base: 50,
        chance_plus: 100,
        chance_per_level: 1,
        ..EffectSettings::default()
    };
    let value100 = chance_value(&c100, 1);
    let value150 = chance_value(&c150, 1);
    let always = (0..=99).all(|roll| {
        roll_chance(true, &c100, 1, roll) && roll_chance(true, &c150, 1, roll)
    });
    rows.push(row(
        "C6",
        format!("value100={value100} value150={value150} always={always}"),
        "value100=100 value150=150 always=true".to_string(),
        "no clamp to 100; anti-false (iv): ChanceValue() == 150 without truncation",
    ));

    rows
}

/// Block B: runtime registration over the whole registry (contract §1/§2.2):
/// the 153 DFU `Effects/` templates — 143 concrete + 10 abstract bases — expand
/// to [`TEMPLATE_COUNT`] keys, because four families declare a `variantCount`
/// (`ElementalResistance`, `MageLight`, `Pacify`, `Poison`). NEGATIVE N-1 (the
/// 17 reference effects of R1) is superseded and kept only as history.
fn effect_rows_b() -> Vec<Row> {
    let mut registered = 0usize;
    let mut collisions = 0usize;
    let mut variant_rows = 0usize;
    let mut keys: BTreeMap<i32, &str> = BTreeMap::new();
    for props in templates() {
        registered += 1;
        if props.variant_count > 1 {
            variant_rows += 1;
        }
        // `MapClassicKey`: key 0 is not mapped (contract §2.2 N-2).
        if props.classic_key != 0 && keys.insert(props.classic_key, props.key).is_some() {
            collisions += 1;
        }
    }
    // Typed variants register one key per `variantIndex` under a shared key
    // prefix (`ElementalResistance-*`, `MageLight-*`, `Pacify-*`, `Poison-*`);
    // each row must declare exactly the number of siblings the registry holds
    // (`EntityEffect.cs:412`, `MageLight.cs:106`, `PacifyEffect.cs:69`,
    // `PoisonEffect.cs:87`).
    let family_of = |key: &'static str| key.split('-').next().unwrap_or(key);
    let mut variant_families: BTreeMap<&'static str, usize> = BTreeMap::new();
    for props in templates().filter(|template| template.variant_count > 1) {
        *variant_families.entry(family_of(props.key)).or_default() += 1;
    }
    let expanded_extra: usize = variant_families.values().map(|size| size - 1).sum();
    let declared_ok = templates()
        .filter(|template| template.variant_count > 1)
        .filter(|template| {
            variant_families[family_of(template.key)] as i32 == template.variant_count
        })
        .count();
    let lookup_ok = templates()
        .filter(|template| template.classic_key != 0)
        .filter(|template| {
            effect_by_classic_key(template.classic_key).is_some()
                && effect_by_key(template.key).is_some()
        })
        .count();
    let lookup_total = templates()
        .filter(|template| template.classic_key != 0)
        .count();
    let concrete = registered - expanded_extra;
    println!(
        "templates_registered={registered} dfu_census=153 (contract §1: 143 concrete + 10 abstract bases)"
    );
    vec![
        row(
            "B-registered",
            format!("templates_registered={registered}"),
            format!("templates_registered={TEMPLATE_COUNT}"),
            "one registry traversal (`templates()`) == `TEMPLATE_COUNT` (§2.2)",
        ),
        row(
            "B-dfu-census",
            format!(
                "dfu_templates=153 concrete_keys={concrete} expanded_keys={expanded_extra}"
            ),
            "dfu_templates=153 concrete_keys=143 expanded_keys=22".to_string(),
            "the 153 source files pair with 143 concrete keys + 22 typed variants",
        ),
        row(
            "B-collisions",
            format!("classic_key_collisions={collisions}"),
            "classic_key_collisions=0".to_string(),
            "no classicKey collision in the runtime registration",
        ),
        row(
            "B-variants",
            format!(
                "variant_families={} variant_rows={variant_rows} declared_consistent={declared_ok}",
                variant_families.len()
            ),
            format!(
                "variant_families=4 variant_rows={variant_rows} declared_consistent={variant_rows}"
            ),
            "4 families × `variantCount` == registered siblings (`EntityEffect.cs:412`)",
        ),
        row(
            "B-lookup",
            format!("resolved={lookup_ok}/{lookup_total}"),
            format!("resolved={lookup_total}/{lookup_total}"),
            "every mapped classicKey and every key resolve (`GetEffectTemplate`, `:630-637`)",
        ),
    ]
}

/// Block C: instance isolation (spec §9 C — two `InstantiateEffect`
/// with different settings share neither `Settings` nor `RoundsRemaining`).
fn effect_rows_c() -> Vec<Row> {
    let mut state = MagicState::default();
    let short = entry("Levitate", dur(1, 1, 1)); // 2 rounds @ level 1
    let long = entry("Levitate", dur(9, 1, 1)); // 10 rounds @ level 1
    let starts = state.cast(&[short, long], 1);
    let first = &state.active()[0];
    let second = &state.active()[1];
    let settings_equal = first.settings == second.settings;
    let row1 = row(
        "C-instances",
        format!(
            "starts={starts:?} settings_equal={settings_equal} dur_base=({}, {})",
            first.settings.duration_base, second.settings.duration_base
        ),
        "starts=[2, 10] settings_equal=false dur_base=(1, 9)".to_string(),
        "each instance isolates its settings (`EntityEffectBroker.cs:660-683`)",
    );
    state.magic_round();
    let (first_left, second_left) = (
        state.active()[0].rounds_remaining,
        state.active()[1].rounds_remaining,
    );
    let row2 = row(
        "C-counters",
        format!("after_round=({first_left}, {second_left})"),
        "after_round=(1, 9)".to_string(),
        "counters are not contaminated across instances",
    );
    vec![row1, row2]
}

/// Block D: the `(classicKey → effectKey)` map built at runtime from the
/// registry (contract §2.2 N-2): every mapped key resolves through
/// `effect_by_classic_key`, and the canonical digest is stable across two
/// independent traversals.
fn effect_map() -> BTreeMap<i32, &'static str> {
    let mut map = BTreeMap::new();
    for props in templates() {
        if props.classic_key != 0 {
            map.entry(props.classic_key).or_insert(props.key);
        }
    }
    map
}

fn effect_rows_d() -> Vec<Row> {
    let canonical = |map: &BTreeMap<i32, &'static str>| {
        let mut text = String::new();
        for (key, name) in map {
            text.push_str(&format!("{key}={name};"));
        }
        fnv1a64(text.as_bytes())
    };
    let map = effect_map();
    let first = canonical(&map);
    let second = canonical(&effect_map());
    let resolved = map
        .keys()
        .filter(|key| {
            effect_by_classic_key(**key)
                .map(|template| template.classic_key == **key)
                .unwrap_or(false)
        })
        .count();
    let absent = templates().filter(|template| template.classic_key == 0).count();
    vec![
        row(
            "D-map-covers-registry",
            format!("resolved={resolved}/{} without_classic_key={absent}", map.len()),
            format!("resolved={}/{} without_classic_key={absent}", map.len(), map.len()),
            "every mapped classicKey resolves through the runtime map",
        ),
        row(
            "D-digest",
            format!("digest={first}"),
            format!("digest={second}"),
            "sorted (classicKey → effectKey) map, digest ×2 identical",
        ),
    ]
}

// ── PROBE-EFFECT-001 · block E · applied mutations + per-key census ────────

/// Scripted dice for the applied rows: `Range` answers the queued values in
/// order (so `GetMagnitude` = base + plus·⌊level/per⌋ is exact) and every
/// `Dice100` answers `d100_value` (0 ⇒ the `OnCast` roll always succeeds).
struct FixedDice {
    ranges: VecDeque<i32>,
    d100_value: i32,
}

impl FixedDice {
    fn new(ranges: &[i32], d100_value: i32) -> Self {
        Self {
            ranges: ranges.iter().copied().collect(),
            d100_value,
        }
    }
}

impl EffectDice for FixedDice {
    fn range_inclusive(&mut self, _low: i32, _high: i32) -> i32 {
        self.ranges.pop_front().unwrap_or(0)
    }

    fn d100(&mut self) -> i32 {
        self.d100_value
    }
}

/// Flat 100/100 target, all attributes 50 (`MaxFatigue = (50 + 50)·64`).
fn applied_target() -> EffectTarget {
    EffectTarget::new(100, 100, [50; 8])
}

/// Duration `(1,1,1)` + magnitude `(10,10,5,5,1)`: N = 2 rounds, magnitude
/// 15 at level 1 with `FixedDice::new(&[10, 5], 0)`.
fn duration_and_magnitude() -> EffectSettings {
    EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        magnitude_base_min: 10,
        magnitude_base_max: 10,
        magnitude_plus_min: 5,
        magnitude_plus_max: 5,
        magnitude_per_level: 1,
        ..EffectSettings::classic_default()
    }
}

/// Duration `(1,1,1)` + chance `(base, plus, perLevel)`.
fn duration_and_chance(base: i32, plus: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        chance_base: base,
        chance_plus: plus,
        chance_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// One cast of a single effect, returning the report and the target.
fn cast_one(
    key: &str,
    settings: EffectSettings,
    target: &mut EffectTarget,
    dice: &mut FixedDice,
) -> (MagicState, CastReport) {
    let mut state = MagicState::default();
    let report = state.cast_effects(&[entry(key, settings)], 1, target, dice);
    (state, report)
}

/// What happened to the duration state a template left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Revert {
    /// The state returned to its pre-cast value when every instance ended.
    Reverted,
    /// The template leaves no duration state (instant, or a bare passive).
    NotApplicable,
    /// `Lifetime::Permanent` — the state is never expected to revert.
    Permanent,
    /// Duration state survived expiry (or did not return) — a real failure.
    Failed,
}

/// One template driven through its own paths on the neutral target.
struct TemplateEvidence {
    lifetime: Lifetime,
    paths: Vec<&'static str>,
    applied: usize,
    /// Magic rounds the template's duration state was ticked through.
    rounds: i32,
    /// `Lifetime::Permanent` only: a live instance still holds after 4 rounds
    /// (`MAGIC_ROUNDS_FOR_PERMANENCE`), i.e. the effect does not expire.
    alive_after_rounds: bool,
    revert: Revert,
    /// Changes the item triggers produced on the **bearer** (`sourceEntity`).
    bearer_changes: usize,
    /// Changes they produced on the **target** (`targetEntity`).
    target_changes: usize,
}

/// Rounds a `Permanent` template is observed alive for (contract §5.2).
const MAGIC_ROUNDS_FOR_PERMANENCE: i32 = 4;

impl TemplateEvidence {
    fn visible(&self) -> bool {
        self.applied > 0
    }

    /// Contract §5.1-5.3: visible on its own path, duration state ticked
    /// through at least one magic round, permanent state alive past the
    /// horizon, and every duration state reverted (or declared permanent).
    ///
    /// Rounds are only due when the template **supports** duration: a
    /// `Duration` lifetime without `SupportDuration` (`Lycanthropy`,
    /// `Vampirism`, `Passive-Specials` — no `EffectSettings` can give it a
    /// round) is applied at `Start` and its payload declares no expiry.
    fn holds(&self, props: &EffectProperties) -> bool {
        self.visible()
            && self.revert != Revert::Failed
            && (!props.support_duration || self.lifetime != Lifetime::Duration || self.rounds >= 1)
            && (self.lifetime != Lifetime::Permanent || self.alive_after_rounds)
    }

    fn summary(&self, props: &EffectProperties) -> String {
        format!(
            "key={} lifetime={:?} paths=[{}] applied={} bearer={} target={} rounds={} alive_after={} revert={:?}",
            props.key,
            self.lifetime,
            self.paths.join(","),
            self.applied,
            self.bearer_changes,
            self.target_changes,
            self.rounds,
            self.alive_after_rounds,
            self.revert
        )
    }
}

/// Neutral target with room in both directions: half-full pools, a worn item
/// at half condition, attribute/skill damage, one disease, one poison and the
/// requested paralysis. Every drain, heal, cure, wear and repair is observable.
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

/// Whether the template clears paralysis (`Payload::Cure(Cure::Paralyzation)`),
/// the only condition that has to be **present** before the cast to be visible.
fn cures_paralysis(props: &EffectProperties) -> bool {
    let clears = |payload: &Payload| matches!(payload, Payload::Cure(Cure::Paralyzation));
    props.payload.iter().any(|payload| match payload {
        Payload::On { action, .. } => action.iter().any(clears),
        single => clears(single),
    })
}

/// Fields whose change is a **consequence** the effect layer never reverts:
/// spent/gained resources, cured disease or poison, item wear, permanent
/// attribute/skill damage, and the world requests the effect emitted.
fn is_consumable(field: &str) -> bool {
    matches!(
        field,
        "health" | "fatigue" | "spell_points" | "disease" | "poison" | "item.condition"
    ) || field.starts_with("request.")
        || field.starts_with("attribute_damage.")
        || field.starts_with("skill_damage.")
}

/// Runs one template through its **own** paths — the gates it declares held,
/// the cast (`Start`), the magic rounds until every instance ends (`MagicRound`),
/// and its declared item triggers (`Payload::On` → `EnchantmentPayloadCallback`)
/// — and reports what changed and whether the duration state reverted.
fn template_evidence(props: &'static EffectProperties) -> TemplateEvidence {
    let mut target = neutral_target(cures_paralysis(props));
    // A cast at level 1 with a real duration (`1 + 1·1`): a duration template
    // gets its rounds, an instant one ignores the setting (`rounds`).
    let settings = EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        ..EffectSettings::classic_default()
    };
    let mut dice = FixedDice::new(&[1, 1, 1, 1], 0);
    let mut state = MagicState::default();
    let mut paths: Vec<&'static str> = Vec::new();
    let mut applied = 0usize;
    let mut owned_state: Vec<EffectChange> = Vec::new();

    // The gates the template declares (top level or inside an item trigger)
    // must hold while it is cast: that is the condition its owner organ checks.
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

    let report = state.cast_effects(&[EffectEntry { props, settings }], 1, &mut target, &mut dice);
    if report.applications[0].chance_passed {
        let cast = &report.applications[0].changes;
        if !cast.is_empty() {
            paths.push("cast");
            applied += cast.len();
        }
        if !report.state_changes.is_empty() {
            paths.push("state");
            applied += report.state_changes.len();
            owned_state.extend(report.state_changes.iter().cloned());
        }
    }

    // Magic rounds until every instance ends (per-round payloads + expiry).
    let mut rounds_ticked = 0;
    let mut alive_after_rounds = false;
    for _ in 0..=64 {
        if state.active().is_empty() {
            break;
        }
        let round = state.magic_round_with(1, &mut target, &mut dice);
        rounds_ticked += 1;
        let round_applied: Vec<EffectChange> = round
            .applications
            .iter()
            .flat_map(|application| application.changes.iter().cloned())
            .collect();
        if !round_applied.is_empty() {
            paths.push("round");
            applied += round_applied.len();
        }
        if !round.state_changes.is_empty() {
            paths.push("state");
            applied += round.state_changes.len();
            owned_state.extend(round.state_changes.iter().cloned());
        }
        if rounds_ticked == MAGIC_ROUNDS_FOR_PERMANENCE {
            alive_after_rounds = !state.active().is_empty();
        }
        if !round.expired.is_empty() {
            break;
        }
    }
    if rounds_ticked < MAGIC_ROUNDS_FOR_PERMANENCE {
        alive_after_rounds = !state.active().is_empty();
    }

    let mut bearer_changes = 0usize;
    let mut target_changes = 0usize;
    // Item triggers: the payload runs as the item owner would run it — the
    // bearer keeps the item, a second neutral entity plays `targetEntity`.
    for payload in props.payload {
        if let Payload::On { trigger, .. } = payload {
            let mut struck = neutral_target(cures_paralysis(props));
            // The owner organ holds the same world gates for both entities.
            struck.gates = target.gates;
            let report = run_enchantment(props, *trigger, 1, &mut target, &mut struck, &mut dice);
            if !report.is_empty() {
                paths.push("trigger");
                bearer_changes += report.bearer.len();
                target_changes += report.struck.len();
                applied += report.bearer.len() + report.struck.len();
            }
        }
    }

    let lifetime = props.lifetime();
    let snapshot = target.observe();
    // The baseline of a field is the `before` of its **first** change: later
    // rounds diff against the state the effect itself raised.
    let mut baseline: BTreeMap<&str, i64> = BTreeMap::new();
    for change in &owned_state {
        baseline.entry(change.field.as_str()).or_insert(change.before);
    }
    let unreverted = baseline
        .iter()
        .filter(|(field, _)| !is_consumable(field))
        .filter(|(field, before)| {
            let current = snapshot
                .iter()
                .find(|(name, _)| name.as_str() == **field)
                .map(|(_, value)| *value)
                .unwrap_or(0);
            current != **before
        })
        .count();
    // `cast_effects` keeps a `Duration` template that declares no rounds
    // (`Lycanthropy-Curse`, `Vampirism-Curse`, `Passive-Specials`) live until
    // the owner removes it, exactly like a `Permanent` one: its instance
    // carries `PERMANENT_ROUNDS`. The classification asks the instance, so a
    // duration that failed to expire can never pass as `Permanent`.
    let kept_until_removed = state.rounds_left(props.key) == PERMANENT_ROUNDS;
    let revert = if lifetime == Lifetime::Permanent || kept_until_removed {
        Revert::Permanent
    } else if owned_state.is_empty() {
        Revert::NotApplicable
    } else if unreverted == 0 && state.active().is_empty() {
        Revert::Reverted
    } else {
        Revert::Failed
    };
    paths.dedup();
    TemplateEvidence {
        lifetime,
        paths,
        applied,
        rounds: rounds_ticked,
        alive_after_rounds,
        revert,
        bearer_changes,
        target_changes,
    }
}

// ── block F · per-payload evidence (contract §5.4-§5.10) ───────

/// Frozen drives of the per-payload detector: every leaf of every registered
/// template is driven alone, with fixed settings and fixed dice, and judged
/// against an expectation written from the DFU side — never from the engine's
/// own output (that would be assertion-fitting).
///
/// `V` = magnitude rolled `(5,5)` with0 plus at level 1
/// (`roll_magnitude`, `EntityEffect.cs:786-800`); `CH` = chance base 100
/// (`chance_value`, `EntityEffect.cs:740-745`); `E` = the trigger's own
/// `sourceDamage` (`VampiricEffect.cs:112-128`); `M` = the neutral worn
/// item's `EnchantmentParam.Value` (`MaceOfMolagBalEffect.cs:99`).
const LEAF_MAGNITUDE: i32 = 5;
const LEAF_CHANCE: i32 = 100;
const LEAF_EVENT: i32 = 10;
const LEAF_ITEM_MAGNITUDE: i32 = 5;
/// Whole resource units the neutral fixture holds on each side (health 50,
/// spell points 50, fatigue 3200/64 = 50 — `neutral_target`): the bound a
/// `TransferResource` trigger can take from the struck entity.
const LEAF_NEUTRAL_UNITS: i32 = 50;
/// Base attribute of the neutral fixture (50) and its permanent damage (20):
/// `DrainAttribute` clamps at `base − 1 − damage` (`DrainEffect.cs:125-137`)
/// and `HealAttribute` cannot heal more damage than there is.
const LEAF_BASE_ATTRIBUTE: i32 = 50;
const LEAF_ATTRIBUTE_DAMAGE: i32 = 20;
/// Max spell points of the neutral fixture (100): the base `MaxMagicka`
/// percentages resolve against (`PassiveSpecialsEffect.cs:218`).
const LEAF_MAX_SPELL_POINTS: i32 = 100;

/// Duration `(1,1,1)` ⇒ 2 rounds at level 1, magnitude `(5,5)` plus 0 ⇒
/// V = 5, chance `(100,0,·)` ⇒ CH = 100. `chance_per_level = 1` (not 0):
/// `chance_value` divides by it (`EntityEffect.cs:740-745`).
fn leaf_settings() -> EffectSettings {
    EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        chance_base: LEAF_CHANCE,
        chance_plus: 0,
        chance_per_level: 1,
        magnitude_base_min: LEAF_MAGNITUDE,
        magnitude_base_max: LEAF_MAGNITUDE,
        magnitude_plus_min: 0,
        magnitude_plus_max: 0,
        magnitude_per_level: 1,
    }
}

/// Dice answering the fixed drives: `5, 0` per magnitude roll (base, plus) —
/// enough for the cast plus the one extra round a per-round leaf ticks — and
/// `d100 = 0`, which always passes (`Dice100.cs:15-18`).
fn leaf_dice() -> FixedDice {
    FixedDice::new(&[5, 0, 5, 0], 0)
}

/// Which entity a leaf drive mutated (the two-entity model of contract §5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LeafEntity {
    /// The single cast target (a spell has one entity in the probe).
    Cast,
    /// `sourceEntity` — the item's bearer.
    Bearer,
    /// `targetEntity` — the struck entity.
    Struck,
}

impl LeafEntity {
    fn name(self) -> &'static str {
        match self {
            LeafEntity::Cast => "cast",
            LeafEntity::Bearer => "bearer",
            LeafEntity::Struck => "struck",
        }
    }
}

/// One payload leaf with the path that reaches it: `On{trigger, action}`
/// containers are unwrapped (the leaf remembers its trigger) and so is
/// `Bearer(..)` (the leaf remembers it acts on `sourceEntity`).
struct LeafRef {
    trigger: Option<Trigger>,
    through_bearer: bool,
    payload: &'static Payload,
}

/// Every leaf of a payload list, in declaration order, recursing through
/// `On{action}` and `Bearer(..)` — the flat set contract §5.4 judges.
fn flatten_leaves(payloads: &'static [Payload]) -> Vec<LeafRef> {
    fn walk(
        payloads: &'static [Payload],
        trigger: Option<Trigger>,
        through_bearer: bool,
        leaves: &mut Vec<LeafRef>,
    ) {
        for payload in payloads {
            match payload {
                Payload::On { trigger: fired, action } => {
                    walk(action, Some(*fired), through_bearer, leaves);
                }
                Payload::Bearer(action) => walk(action, trigger, true, leaves),
                leaf => leaves.push(LeafRef {
                    trigger,
                    through_bearer,
                    payload: leaf,
                }),
            }
        }
    }
    let mut leaves = Vec::new();
    walk(payloads, None, false, &mut leaves);
    leaves
}

/// Declared residuals (contract §5.6/§6): leaves the single-entity model
/// cannot observe, so their rows demand an EMPTY observation and go red the
/// day one fires.
///
/// * `HealCaster` — the caster half of a `Transfer-*` spell needs a second
///   entity (`TransferEffect.cs:37-43`; M25 owns entities). On the item path
///   it is observable and §5.7 pins its entity.
/// * `HealthLimiter` — DFU computes the limiter dynamically per minute
///   (`RawMaxHealth − elapsed·24/min`, floor 4, `LycanthropyEffect.cs:232-237`);
///   the template declares no `SupportMagnitude`, so the `Rolled` payload has
///   no value to resolve and stays at the observable zero.
fn is_declared_residual(payload: &Payload) -> bool {
    matches!(payload, Payload::HealCaster(_) | Payload::HealthLimiter(_))
}

/// The entity-scope table of contract §5.7, written from DFU — deliberately
/// **not** derived from `magic.rs` (its `trigger_carries_target` /
/// `payload_is_bearer_scoped` are private, and reading them would make the
/// expectation fit the engine):
///
/// * `TriggerAmount` / `ItemCondition` / `HealCaster` name `sourceEntity`
///   even on a strike (`HealthLeech.cs:88-89`, `VampiricEffect.cs:127`,
///   `TransferEffect.cs:41`);
/// * `TransferResource` crosses both entities (`MaceOfMolagBalEffect.cs:113-126`);
/// * `Held` / `Equipped` / `Used` / `Enchanted` / `Breaks` / `MagicRound` /
///   `Reroll` pass `null` as target and act on the bearer
///   (`EntityEffectManager.cs:1040/1090/1120/1127/1134`);
/// * `Strikes` carries both (`EntityEffectManager.cs:1104`,
///   `WeaponManager.cs:618`, `EnemyAttack.cs:267,382`), `Struck` points the
///   target at the attacker (`FormulaHelper.cs:704-719`).
fn dfu_table_entities(leaf: &LeafRef) -> Vec<LeafEntity> {
    if leaf.through_bearer {
        return vec![LeafEntity::Bearer];
    }
    match leaf.payload {
        Payload::TriggerAmount { .. }
        | Payload::ItemCondition { .. }
        | Payload::HealCaster(_) => vec![LeafEntity::Bearer],
        Payload::TransferResource(_) => vec![LeafEntity::Struck, LeafEntity::Bearer],
        _ => match leaf.trigger {
            Some(Trigger::Strikes | Trigger::Struck) => vec![LeafEntity::Struck],
            _ => vec![LeafEntity::Bearer],
        },
    }
}

/// Resource payloads speak whole units; fatigue is stored in 1/64
/// (`DaggerfallEntity.cs:340-348`).
fn leaf_unit(resource: Resource) -> i32 {
    match resource {
        Resource::Fatigue => FATIGUE_MULTIPLIER,
        Resource::Health | Resource::SpellPoints => 1,
    }
}

/// `resolve_amount` (`PassiveSpecialsEffect.cs` `RawMaxMaxicka · −0.33f`,
/// `EntityEffect.cs` `GetMagnitude`): `Rolled` is the frozen magnitude,
/// `Fixed` the template constant, `Percent` a percentage of the base.
fn leaf_resolve(amount: Amount, magnitude: i32, base: i32) -> i32 {
    match amount {
        Amount::Rolled => magnitude,
        Amount::Fixed(value) => value,
        Amount::Percent(percent) => base * percent / 100,
    }
}

/// The world gate a leaf declares is held for its drive (the owner organ
/// resolves it before the cast — same rule as [`template_evidence`], narrowed
/// to the driven leaf).
fn set_leaf_gates(leaf: &LeafRef, target: &mut EffectTarget) {
    if let Payload::GatedDamage { gate, .. } = leaf.payload {
        target.gates.set(*gate, true);
    }
}

/// Whether the leaf clears paralysis (`Payload::Cure(Cure::Paralyzation)`):
/// the one condition the fixture must PRESENT for the drive to observe the
/// cure.
fn leaf_cures_paralysis(leaf: &LeafRef) -> bool {
    matches!(leaf.payload, Payload::Cure(Cure::Paralyzation))
}

/// Drives one leaf through the path it declares and returns the exact
/// observation diff per entity: the narrowed cast (`MagicState::cast_leaf_effect`
/// — the template's chance gate and settings, ONE payload applied — plus one
/// magic round when the leaf rolls its magnitude per round, DFU's per-round
/// cadence) or the item trigger (`run_enchantment_leaf`, the callback's own
/// routing) with [`LEAF_EVENT`] as `sourceDamage`.
fn drive_leaf(
    props: &'static EffectProperties,
    leaf: &LeafRef,
) -> (BTreeMap<(LeafEntity, String), i64>, Vec<LeafEntity>) {
    let mut observed: BTreeMap<(LeafEntity, String), i64> = BTreeMap::new();
    match leaf.trigger {
        None => {
            let mut target = neutral_target(leaf_cures_paralysis(leaf));
            set_leaf_gates(leaf, &mut target);
            let before = target.observe();
            let mut dice = leaf_dice();
            let mut state = MagicState::default();
            state.cast_leaf_effect(props, leaf.payload, &leaf_settings(), 1, &mut target, &mut dice);
            if matches!(leaf.payload, Payload::ResourcePerRound { .. }) {
                // A per-round payload applies at the cast and on every magic
                // round (`DamageHealth.cs:53`): one extra round makes the
                // cadence observable (want = 2 × sign × V).
                state.magic_round_with(1, &mut target, &mut dice);
            }
            for change in diff_observations(&before, &target.observe()) {
                observed.insert(
                    (LeafEntity::Cast, change.field),
                    change.after - change.before,
                );
            }
        }
        Some(trigger) => {
            let mut bearer = neutral_target(leaf_cures_paralysis(leaf));
            set_leaf_gates(leaf, &mut bearer);
            let mut struck = neutral_target(leaf_cures_paralysis(leaf));
            // The owner organ holds the same world gates for both entities.
            struck.gates = bearer.gates;
            let mut dice = leaf_dice();
            let report = run_enchantment_leaf(
                props.key,
                leaf.payload,
                leaf.through_bearer,
                trigger,
                LEAF_EVENT,
                LEAF_ITEM_MAGNITUDE,
                &mut bearer,
                &mut struck,
                &mut dice,
            );
            for change in report.bearer {
                observed.insert(
                    (LeafEntity::Bearer, change.field),
                    change.after - change.before,
                );
            }
            for change in report.struck {
                observed.insert(
                    (LeafEntity::Struck, change.field),
                    change.after - change.before,
                );
            }
        }
    }
    let mut entities: Vec<LeafEntity> = observed.keys().map(|(entity, _)| *entity).collect();
    entities.dedup();
    (observed, entities)
}

/// The DFU-side expectation of one leaf drive — `(entity, field, delta)`
/// triples written only from the DFU sources cited per payload, never from
/// the engine's output. `None` declares the payload residual
/// ([`is_declared_residual`]); a zero delta is not observable, so an
/// expectation whose every delta is zero is exactly a dead payload and the
/// applied-class rule turns it RED (contract §5.5 — the rule that would have
/// caught `Passive-Specials`' inert regeneration).
fn leaf_expectation(leaf: &LeafRef) -> Option<Vec<(LeafEntity, String, i32)>> {
    // An item payload scales with the worn item's magnitude (M); a cast rolls
    // the settings' magnitude (V). Both drives freeze at the same value.
    let magnitude = if leaf.trigger.is_none() {
        LEAF_MAGNITUDE
    } else {
        LEAF_ITEM_MAGNITUDE
    };
    // Cast payloads land on the single cast target; trigger payloads follow
    // the entity table of contract §5.7.
    let entity = match leaf.trigger {
        None => LeafEntity::Cast,
        Some(_) => dfu_table_entities(leaf)[0],
    };
    let entries = match leaf.payload {
        Payload::Resource { resource, sign } => vec![(
            entity,
            resource.name().to_string(),
            i32::from(*sign) * magnitude * leaf_unit(*resource),
        )],
        // The cast applies once and the one ticked magic round applies once
        // (`ContinuousDamage*.cs:56`, `Regenerate.cs:66-76`).
        Payload::ResourcePerRound { resource, sign } => vec![(
            entity,
            resource.name().to_string(),
            2 * i32::from(*sign) * magnitude * leaf_unit(*resource),
        )],
        Payload::FixedResource {
            resource,
            sign,
            amount,
        } => vec![(
            entity,
            resource.name().to_string(),
            i32::from(*sign) * *amount * leaf_unit(*resource),
        )],
        // `ItemDeteriorates.cs:72-104`: clamped to `[0, modified max]`.
        Payload::ItemCondition { delta } => {
            let worn = LEAF_NEUTRAL_UNITS; // neutral condition 50 of max 100
            vec![(
                LeafEntity::Bearer,
                "item.condition".to_string(),
                (worn + *delta).clamp(0, 2 * worn) - worn,
            )]
        }
        // The trigger's own argument; a spell cast carries none (`E = 0`), so
        // a top-level `TriggerAmount` can never satisfy its own expectation.
        Payload::TriggerAmount { resource, sign } => vec![(
            entity,
            resource.name().to_string(),
            i32::from(*sign) * LEAF_EVENT * leaf_unit(*resource),
        )],
        Payload::HealAttribute(attribute) => {
            let heal = LEAF_MAGNITUDE.min(LEAF_ATTRIBUTE_DAMAGE);
            let mut out = vec![
                (
                    entity,
                    format!("attribute_damage.{}", attribute.name()),
                    -heal,
                ),
                (entity, format!("attribute.{}", attribute.name()), heal),
            ];
            // The live attribute moves `MaxFatigue = (LiveStr + LiveEnd)·64`
            // (`DaggerfallEntity.cs:262`).
            if matches!(attribute, Attribute::Strength | Attribute::Endurance) {
                out.push((entity, "max_fatigue".to_string(), heal * FATIGUE_MULTIPLIER));
            }
            out
        }
        Payload::DrainAttribute(attribute) => {
            let drain = LEAF_MAGNITUDE.min(LEAF_BASE_ATTRIBUTE - 1 - LEAF_ATTRIBUTE_DAMAGE);
            let mut out = vec![
                (
                    entity,
                    format!("attribute_damage.{}", attribute.name()),
                    drain,
                ),
                (entity, format!("attribute.{}", attribute.name()), -drain),
            ];
            if matches!(attribute, Attribute::Strength | Attribute::Endurance) {
                out.push((entity, "max_fatigue".to_string(), -drain * FATIGUE_MULTIPLIER));
            }
            out
        }
        Payload::TransferResource(resource) => {
            let unit = leaf_unit(*resource);
            if leaf.trigger.is_none() {
                // The single-entity cast model heals the target it drains: a
                // declared divergence (MINOR-3, contract §6), not the two
                // entities of the item path.
                vec![(LeafEntity::Cast, resource.name().to_string(), magnitude * unit)]
            } else {
                // `MaceOfMolagBalEffect.cs:113-126`: the victim loses what it
                // had (bounded by the fixture) and the bearer gains exactly
                // that.
                let taken = magnitude.min(LEAF_NEUTRAL_UNITS);
                vec![
                    (LeafEntity::Struck, resource.name().to_string(), -taken * unit),
                    (LeafEntity::Bearer, resource.name().to_string(), taken * unit),
                ]
            }
        }
        Payload::FortifyAttribute(attribute) => {
            let live = LEAF_MAGNITUDE;
            let mut out = vec![
                (
                    entity,
                    format!("attribute_mod.{}", attribute.name()),
                    live,
                ),
                (entity, format!("attribute.{}", attribute.name()), live),
            ];
            // `MaxFatigue = (LiveStr + LiveEnd) · 64` moves with the live
            // attribute (`DaggerfallEntity.cs:262`).
            if matches!(attribute, Attribute::Strength | Attribute::Endurance) {
                out.push((entity, "max_fatigue".to_string(), live * FATIGUE_MULTIPLIER));
            }
            out
        }
        Payload::MaxAttribute(attribute, amount) => {
            let base = LEAF_BASE_ATTRIBUTE;
            let live = leaf_resolve(*amount, magnitude, base);
            let mut out = vec![
                (
                    entity,
                    format!("attribute_max_mod.{}", attribute.name()),
                    live,
                ),
                (entity, format!("attribute.{}", attribute.name()), live),
            ];
            if matches!(attribute, Attribute::Strength | Attribute::Endurance) {
                out.push((entity, "max_fatigue".to_string(), live * FATIGUE_MULTIPLIER));
            }
            out
        }
        Payload::Skill(skill, amount) => {
            let live = leaf_resolve(*amount, magnitude, 0);
            vec![
                (entity, format!("skill_mod.{}", skill.name()), live),
                (entity, format!("skill.{}", skill.name()), live),
            ]
        }
        Payload::Resistance(element, amount) => {
            // The chance value IS the resistance (`ElementalResistance.cs:50-54`).
            let live = leaf_resolve(*amount, LEAF_CHANCE, 0);
            vec![
                (entity, format!("resistance.{}", element.name()), live),
                (
                    entity,
                    format!("resistance_flag.{}", element.name()),
                    1,
                ),
            ]
        }
        Payload::MaxMagicka(amount) => {
            let live = leaf_resolve(*amount, magnitude, LEAF_MAX_SPELL_POINTS);
            vec![
                (entity, "max_magicka_mod".to_string(), live),
                (
                    entity,
                    "max_spell_points".to_string(),
                    (LEAF_MAX_SPELL_POINTS + live).max(0) - LEAF_MAX_SPELL_POINTS,
                ),
            ]
        }
        Payload::ArmorValue(delta) => {
            vec![(entity, "armor_value_mod".to_string(), *delta)]
        }
        Payload::WeightAllowance(percent) => {
            // The neutral multiplier is 100 (`ItemState::default`).
            vec![(entity, "weight_allowance".to_string(), *percent - 100)]
        }
        Payload::Reaction(group, amount) => {
            let live = leaf_resolve(*amount, magnitude, 0);
            let groups = if *group == SocialGroup::All {
                &SocialGroup::GROUPS[..]
            } else {
                std::slice::from_ref(group)
            };
            groups
                .iter()
                .map(|candidate| {
                    (entity, format!("reaction.{}", candidate.name()), live)
                })
                .collect()
        }
        Payload::ChanceToHit(amount) => {
            vec![(entity, "chance_to_hit_mod".to_string(), leaf_resolve(*amount, magnitude, 0))]
        }
        Payload::Flag(flag) => vec![(entity, flag.name().to_string(), 1)],
        Payload::Concealment(flag) => {
            vec![(entity, format!("concealment.{}", flag.name()), 1)]
        }
        Payload::ChanceState(state) => {
            vec![(entity, format!("chance.{}", state.name()), LEAF_CHANCE)]
        }
        Payload::Cure(cure) => {
            let field = match cure {
                Cure::Disease => "disease",
                Cure::Poison => "poison",
                Cure::Paralyzation => Flag::Paralyzed.name(),
            };
            vec![(LeafEntity::Cast, field.to_string(), -1)]
        }
        Payload::Shield => vec![(entity, "shield".to_string(), magnitude)],
        Payload::Request(request) => {
            vec![(entity, format!("request.{}", request.label()), 1)]
        }
        Payload::Versus { group, amount } => {
            vec![(entity, format!("versus.{}", group.name()), *amount)]
        }
        Payload::GatedDamage { amount, .. } => {
            // The drive holds every declared gate, so the damage always lands.
            vec![(entity, "health".to_string(), -(*amount))]
        }
        Payload::Disease(_) => vec![(entity, "disease".to_string(), 1)],
        Payload::Poison(_) => vec![(entity, "poison".to_string(), 1)],
        // Declared residuals: no expectation — the row demands an empty
        // observation ([`is_declared_residual`]).
        Payload::HealCaster(_) | Payload::HealthLimiter(_) => return None,
        // Containers are never leaves ([`flatten_leaves`] unwraps them); the
        // arm keeps the match exhaustive so a vocabulary addition fails to
        // compile here instead of passing silently.
        Payload::On { .. } | Payload::Bearer(_) => Vec::new(),
    };
    Some(entries)
}

/// The expectation as the exact comparison map: zero deltas are dropped (they
/// cannot be observed), so a leaf whose whole expectation computes to zero is
/// a dead payload and fails the applied-class rule.
fn leaf_expected_map(leaf: &LeafRef) -> BTreeMap<(LeafEntity, String), i64> {
    leaf_expectation(leaf)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, _, delta)| *delta != 0)
        .map(|(entity, field, delta)| ((entity, field), i64::from(delta)))
        .collect()
}

/// Lifecycle class of one leaf row (contract §5.5-§5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeafClass {
    /// `observed == expected`, exactly, or the row is RED.
    Applied,
    /// Declared residual: the observation must be EMPTY, or the row is RED.
    Residual,
}

/// One leaf driven and judged (contract §5.5).
struct LeafEvidence {
    class: LeafClass,
    ok: bool,
    got: String,
    want: String,
    trigger: Option<Trigger>,
    /// Entities the drive actually changed.
    observed_entities: Vec<LeafEntity>,
    /// Entities the DFU table of contract §5.7 says must change.
    table_entity: Vec<LeafEntity>,
}

fn format_delta_map(map: &BTreeMap<(LeafEntity, String), i64>) -> String {
    map.iter()
        .map(|((entity, field), delta)| format!("{}.{}={delta:+}", entity.name(), field))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drives one leaf and applies the falsable verdict rule.
fn leaf_evidence(props: &'static EffectProperties, leaf: &LeafRef) -> LeafEvidence {
    let (observed, observed_entities) = drive_leaf(props, leaf);
    let table_entity = dfu_table_entities(leaf);
    let path = leaf
        .trigger
        .map_or("cast".to_string(), |trigger| trigger.name().to_string());
    if is_declared_residual(leaf.payload) {
        return LeafEvidence {
            class: LeafClass::Residual,
            ok: observed.is_empty(),
            got: format!("residual via={path} observed {{{}}}", format_delta_map(&observed)),
            want: format!("residual via={path} observed {{}}"),
            trigger: leaf.trigger,
            observed_entities,
            table_entity,
        };
    }
    let expected = leaf_expected_map(leaf);
    LeafEvidence {
        class: LeafClass::Applied,
        ok: !expected.is_empty() && observed == expected,
        got: format!("observed {{{}}}", format_delta_map(&observed)),
        want: format!("expected {{{}}}", format_delta_map(&expected)),
        trigger: leaf.trigger,
        observed_entities,
        table_entity,
    }
}

/// The `F-p-<key>-<i>` row of one leaf.
fn leaf_row(
    props: &EffectProperties,
    leaf: &LeafRef,
    index: usize,
    evidence: &LeafEvidence,
) -> Row {
    let class = match evidence.class {
        LeafClass::Applied => "applied",
        LeafClass::Residual => "residual",
    };
    Row {
        id: format!("F-p-{}-{index}", props.key),
        ok: evidence.ok,
        got: evidence.got.clone(),
        want: evidence.want.clone(),
        detail: format!(
            "leaf={class} via={} classicKey={} payload={:?}",
            leaf.trigger
                .map_or("cast".to_string(), |trigger| trigger.name().to_string()),
            props.classic_key,
            leaf.payload
        ),
    }
}

/// The `F-e-<key>` row of one item-trigger template: the set of entities its
/// leaves actually changed vs the set the DFU scope table of contract §5.7
/// declares for those same leaves.
fn trigger_template_row(props: &EffectProperties, evidences: &[LeafEvidence]) -> Row {
    let mut expected: Vec<LeafEntity> = Vec::new();
    let mut observed: Vec<LeafEntity> = Vec::new();
    for evidence in evidences.iter().filter(|evidence| evidence.trigger.is_some()) {
        if evidence.class != LeafClass::Applied {
            continue;
        }
        for entity in &evidence.table_entity {
            if !expected.contains(entity) {
                expected.push(*entity);
            }
        }
        for entity in &evidence.observed_entities {
            if !observed.contains(entity) {
                observed.push(*entity);
            }
        }
    }
    expected.sort();
    observed.sort();
    let names = |entities: &[LeafEntity]| {
        entities
            .iter()
            .map(|entity| entity.name())
            .collect::<Vec<_>>()
            .join("+")
    };
    row(
        format!("F-e-{}", props.key),
        format!("entities=[{}]", names(&observed)),
        format!("entities=[{}]", names(&expected)),
        "observed entities vs the DFU scope table (contract §5.7: EntityEffectManager.cs:1040/1090/1120/1127/1134/1104, WeaponManager.cs:618, EnemyAttack.cs:267,382, FormulaHelper.cs:704-719, HealthLeech.cs:88-89, VampiricEffect.cs:127, TransferEffect.cs:41, MaceOfMolagBalEffect.cs:113-126)",
    )
}

/// `Cure(Cure::Disease)` on the neutral fixture (`CureDisease.cs:85` clears
/// every disease): with the disease present the payload applies; with it
/// absent the very same payload cannot change anything observable — the exact
/// signature of a dead payload. The pair of rows (contract §5.8) proves the
/// detector discriminates and can never be vacuous: an applied-class
/// expectation nothing can satisfy is RED.
const NEG_CURE_LEAF: Payload = Payload::Cure(Cure::Disease);
const NEG_CURE_DISEASE: EffectProperties = EffectProperties::new(
    "F-neg-cure-disease",
    0,
    MagicSkill::None,
    &[NEG_CURE_LEAF],
);

fn f_neg_cure_disease(present: bool) -> Row {
    let mut target = neutral_target(false);
    if !present {
        target.diseases.clear();
    }
    let before = target.observe();
    let mut dice = leaf_dice();
    let mut state = MagicState::default();
    state.cast_leaf_effect(
        &NEG_CURE_DISEASE,
        &NEG_CURE_LEAF,
        &leaf_settings(),
        1,
        &mut target,
        &mut dice,
    );
    let delta: i64 = diff_observations(&before, &target.observe())
        .iter()
        .find(|change| change.field == "disease")
        .map_or(0, |change| change.after - change.before);
    let applied = delta == -1;
    Row {
        id: if present {
            "F-neg-cure-disease-present".to_string()
        } else {
            "F-neg-cure-disease-absent".to_string()
        },
        // The row asserts the detector's verdict on this fixture: applied on
        // the present one, RED on the absent one (no observation can satisfy
        // the expectation — the dead-payload signature).
        ok: applied == present,
        got: format!(
            "detector={} disease={delta:+}",
            if applied { "applied" } else { "RED" }
        ),
        want: format!(
            "detector={} disease={:+}",
            if present { "applied" } else { "RED" },
            if present { -1 } else { 0 }
        ),
        detail: "negative control: the same payload on a fixture with / without the disease (contract §5.8)"
            .to_string(),
    }
}

/// Entity-inversion antibody (contract §5.10): the three production
/// assertions that fail when a strike's enchantment entities are swapped,
/// evaluated against `run_enchantment` with correct and swapped arguments —
/// `crates/combat/src/enchantments.rs:195-196` (the leech bearer pays 8, the
/// victim is untouched), `:137-138` (the vampiric attacker heals the resolved
/// damage) and `crates/app/tests/combat_runtime.rs:451-463` (the player pays
/// 8 and the enemy takes the strike damage only, i.e. the callback itself
/// takes nothing from it).
fn f_mut_entity_inversion() -> Row {
    let leech = effect_by_key("HealthLeech").expect("registered row");
    let vampiric = effect_by_key("VampiricEffect").expect("registered row");

    // One `Strikes` callback; `swapped` puts the enchanted item on the
    // defender, reproducing the entity inversion the antibodies guard.
    let strike = |weapon: &'static EffectProperties, event: i32, swapped: bool| -> (i32, i32) {
        let mut attacker = neutral_target(false);
        let mut defender = neutral_target(false);
        let mut dice = FixedDice::new(&[], 0);
        if swapped {
            run_enchantment(
                weapon,
                Trigger::Strikes,
                event,
                &mut defender,
                &mut attacker,
                &mut dice,
            );
        } else {
            run_enchantment(
                weapon,
                Trigger::Strikes,
                event,
                &mut attacker,
                &mut defender,
                &mut dice,
            );
        }
        (attacker.health, defender.health)
    };

    let (attacker, defender) = strike(leech, 10, false);
    let calm = i32::from(attacker == 42 && defender == 50);
    let (attacker, defender) = strike(vampiric, 12, false);
    let calm = calm + i32::from(attacker == 62 && defender == 50);
    let (attacker, defender) = strike(leech, 10, false);
    let calm = calm + i32::from(50 - attacker == 8 && defender == 50);

    let (attacker, defender) = strike(leech, 10, true);
    let trip = i32::from(!(attacker == 42 && defender == 50));
    let (attacker, defender) = strike(vampiric, 12, true);
    let trip = trip + i32::from(!(attacker == 62 && defender == 50));
    let (attacker, defender) = strike(leech, 10, true);
    let trip = trip + i32::from(!(50 - attacker == 8 && defender == 50));

    row(
        "F-mut-entity-inversion",
        format!("correct={calm}calm swap={trip}trip"),
        "correct=3calm swap=3trip".to_string(),
        "the three anticuerpo assertions hold with bearer=attacker and trip with the entities swapped (contract §5.10)",
    )
}

/// Total payload leaves of [`templates`] — frozen (contract §5.9), not
/// re-measured at runtime: the `F-payload-census` row compares the live count
/// against it.
const LEAF_PAYLOAD_COUNT: usize = 263;
/// Registered templates declaring at least one item-trigger leaf — frozen
/// (contract §5.9).
const F_TRIGGER_TEMPLATE_COUNT: usize = 33;
/// Leaves declared residual ([`is_declared_residual`]): 8 `HealCaster` +
/// 1 `HealthLimiter` (contract §5.6).
const DECLARED_RESIDUAL_COUNT: usize = 9;

/// Block F — the per-template evidence of contract §5: every registered key
/// through its own path, plus the duration/permanent horizon checks.
fn effect_rows_f() -> Vec<Row> {
    let mut census: Vec<(&'static EffectProperties, TemplateEvidence)> = Vec::new();
    let mut silent = Vec::new();
    for props in templates() {
        let evidence = template_evidence(props);
        if !evidence.visible() {
            silent.push(props.key);
        }
        census.push((props, evidence));
    }
    let mut rows = Vec::with_capacity(
        census.len() + LEAF_PAYLOAD_COUNT + F_TRIGGER_TEMPLATE_COUNT + 8,
    );
    let outcomes = census.len() - silent.len();
    rows.push(row(
        "F-all-templates-act",
        format!("templates={} outcomes={outcomes} silent={silent:?}", census.len()),
        format!("templates={TEMPLATE_COUNT} outcomes={TEMPLATE_COUNT} silent=[]"),
        "every registered template produces a visible mutation or state change",
    ));
    for (props, evidence) in &census {
        rows.push(Row {
            id: format!("F-{}", props.key),
            ok: evidence.holds(props),
            got: evidence.summary(props),
            want: "applied>0 ∧ duration(supported)⇒rounds≥1 ∧ permanent⇒alive ∧ revert∈{ok,n/a,permanent}"
                .to_string(),
            detail: format!(
                "classicKey={} skill={:?} lifetime={:?}",
                props.classic_key, props.magic_skill, evidence.lifetime
            ),
        });
    }
    let reverted = census
        .iter()
        .filter(|(_, evidence)| evidence.revert == Revert::Reverted)
        .count();
    let permanent = census
        .iter()
        .filter(|(_, evidence)| evidence.revert == Revert::Permanent)
        .count();
    let failed = census
        .iter()
        .filter(|(_, evidence)| evidence.revert == Revert::Failed)
        .count();
    let still_alive = census
        .iter()
        .filter(|(_, evidence)| evidence.revert == Revert::Permanent && evidence.alive_after_rounds)
        .count();
    rows.push(Row {
        id: "F-census".to_string(),
        // The invariants, not an echo of the measurement: every registered key
        // has evidence, no duration state leaked, and every template that
        // declared a never-expiring state still holds its instance.
        ok: census.len() == TEMPLATE_COUNT
            && outcomes == TEMPLATE_COUNT
            && failed == 0
            && still_alive == permanent,
        got: format!(
            "keys={} with_evidence={outcomes} revert_ok={reverted} permanent={permanent} permanent_alive={still_alive} revert_failed={failed}",
            census.len()
        ),
        want: format!(
            "keys={TEMPLATE_COUNT} with_evidence={TEMPLATE_COUNT} revert_failed=0 permanent_alive=permanent"
        ),
        detail: "one evidence row per registered key (contract §5): cast/state/round/trigger, applied>0, lifetime horizon, revert"
            .to_string(),
    });

    // Per-payload evidence (contract §5.4-§5.10): one `F-p` row per leaf of
    // every registered template, one `F-e` entity-scope row per item-trigger
    // template, the `F-neg` negative-control pair, the `F-mut` inversion
    // antibody and the frozen per-payload census.
    let mut leaves_total = 0usize;
    let mut applied = 0usize;
    let mut residual = 0usize;
    let mut dead = 0usize;
    let mut trigger_templates = 0usize;
    for props in templates() {
        let leaves = flatten_leaves(props.payload);
        let mut evidences = Vec::with_capacity(leaves.len());
        for (index, leaf) in leaves.iter().enumerate() {
            let evidence = leaf_evidence(props, leaf);
            rows.push(leaf_row(props, leaf, index, &evidence));
            leaves_total += 1;
            match evidence.class {
                LeafClass::Applied if evidence.ok => applied += 1,
                LeafClass::Applied => dead += 1,
                LeafClass::Residual => residual += 1,
            }
            evidences.push(evidence);
        }
        if leaves.iter().any(|leaf| leaf.trigger.is_some()) {
            trigger_templates += 1;
            rows.push(trigger_template_row(props, &evidences));
        }
    }
    rows.push(f_neg_cure_disease(true));
    rows.push(f_neg_cure_disease(false));
    rows.push(f_mut_entity_inversion());
    rows.push(Row {
        id: "F-payload-census".to_string(),
        // The invariants, frozen — never an echo of the measurement
        // (contract §5.9).
        ok: leaves_total == LEAF_PAYLOAD_COUNT
            && trigger_templates == F_TRIGGER_TEMPLATE_COUNT
            && residual == DECLARED_RESIDUAL_COUNT
            && applied == LEAF_PAYLOAD_COUNT - DECLARED_RESIDUAL_COUNT
            && dead == 0,
        got: format!(
            "leaves={leaves_total} trigger_templates={trigger_templates} applied={applied} residual={residual} dead={dead}"
        ),
        want: format!(
            "leaves={LEAF_PAYLOAD_COUNT} trigger_templates={F_TRIGGER_TEMPLATE_COUNT} applied={} residual={DECLARED_RESIDUAL_COUNT} dead=0",
            LEAF_PAYLOAD_COUNT - DECLARED_RESIDUAL_COUNT
        ),
        detail: "frozen per-payload census (contract §5.9): every leaf applied or declared residual, none dead"
            .to_string(),
    });
    rows
}

/// Rows of the applied layer: before/after per effect family, DFU math, and
/// negative controls (expiry clears, failed chance drops, unknown key visible).
fn effect_rows_e() -> Vec<Row> {
    let mut rows = Vec::with_capacity(14);

    // E1: damage subtracts the rolled magnitude.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[10, 5], 0);
    let (_, report) = cast_one("Damage-Health", mag(10, 10, 5, 5, 1), &mut target, &mut dice);
    rows.push(row(
        "E1-damage",
        format!(
            "health=100->{} magnitude={}",
            target.health, report.applications[0].magnitude
        ),
        "health=100->85 magnitude=15".to_string(),
        "10 + 5·⌊1/1⌋ = 15 (`EntityEffect.cs:786-800`) → DamageHealthFromSource",
    ));

    // E2: heal clamps at MaxHealth.
    let mut target = applied_target();
    target.health = 95;
    let mut dice = FixedDice::new(&[10, 5], 0);
    cast_one("Heal-Health", mag(10, 10, 5, 5, 1), &mut target, &mut dice);
    rows.push(row(
        "E2-heal-clamp",
        format!("health=95->{}", target.health),
        "health=95->100".to_string(),
        "IncreaseHealth clamps at MaxHealth (`DaggerfallEntity.cs:331-338`)",
    ));

    // E3: fatigue is stored in 1/64 units.
    let mut target = applied_target();
    target.fatigue = 1_000;
    let mut dice = FixedDice::new(&[10, 5], 0);
    cast_one("Heal-Fatigue", mag(10, 10, 5, 5, 1), &mut target, &mut dice);
    rows.push(row(
        "E3-fatigue",
        format!("fatigue=1000->{} max={}", target.fatigue, target.max_fatigue()),
        "fatigue=1000->1960 max=6400".to_string(),
        "IncreaseFatigue(magnitude, true) ⇒ 15·64 (`DaggerfallEntity.cs:340-348`)",
    ));

    // E4: spell points.
    let mut target = applied_target();
    target.spell_points = 50;
    let mut dice = FixedDice::new(&[10, 5], 0);
    cast_one("Heal-SpellPoints", mag(10, 10, 5, 5, 1), &mut target, &mut dice);
    rows.push(row(
        "E4-spellpoints",
        format!("spell_points=50->{}", target.spell_points),
        "spell_points=50->65".to_string(),
        "IncreaseMagicka (`HealSpellPoints.cs:63-64`)",
    ));

    // E5: fortify raises the modifier and expiry reverts it.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[10, 5], 0);
    let (mut state, _) = cast_one("Fortify-Strength", duration_and_magnitude(), &mut target, &mut dice);
    let fortified = target.effective_attribute(Attribute::Strength);
    state.magic_round_with(1, &mut target, &mut dice);
    let end = state.magic_round_with(1, &mut target, &mut dice);
    rows.push(row(
        "E5-fortify-expiry",
        format!(
            "strength=50->{fortified} after_expiry={} expired={:?}",
            target.effective_attribute(Attribute::Strength),
            end.expired
        ),
        "strength=50->65 after_expiry=50 expired=[\"Fortify-Strength\"]".to_string(),
        "`ChangeStatMod(Strength, GetMagnitude())` (`FortifyEffect.cs:33`), End reverts",
    ));

    // E6: elemental resistance uses ChanceValue as resistance and clears it.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    let (mut state, _) = cast_one(
        "ElementalResistance-Fire",
        duration_and_chance(10, 5, 1),
        &mut target,
        &mut dice,
    );
    let live = format!(
        "resistance={} flag={}",
        target.resistances[Element::Fire as usize],
        target.resistance_flags[Element::Fire as usize]
    );
    state.magic_round_with(1, &mut target, &mut dice);
    state.magic_round_with(1, &mut target, &mut dice);
    rows.push(row(
        "E6-elemental-resistance",
        format!(
            "live({live}) after_expiry=({}, {})",
            target.resistances[Element::Fire as usize],
            target.resistance_flags[Element::Fire as usize]
        ),
        "live(resistance=15 flag=true) after_expiry=(0, false)".to_string(),
        "chance 10+5·⌊1/1⌋ = 15 → RaiseResistanceChance (`ElementalResistance.cs:206-207`)",
    ));

    // E7: spell resistance flag/chance until expiry.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    let (mut state, _) = cast_one("SpellResistance", duration_and_chance(20, 0, 1), &mut target, &mut dice);
    let live = target.chance_state(ChanceState::SpellResistance);
    state.magic_round_with(1, &mut target, &mut dice);
    state.magic_round_with(1, &mut target, &mut dice);
    rows.push(row(
        "E7-spell-resistance",
        format!(
            "live(chance={live}) after_expiry=chance={}",
            target.chance_state(ChanceState::SpellResistance)
        ),
        "live(chance=20) after_expiry=chance=0".to_string(),
        "`Payload::ChanceState` (`SpellResistance.cs:22-36`); `TryResistance` rolls it, End clears it",
    ));

    // E8: the FOUR M19 flags, driven through `movement_effects()`
    // (`MovementEffects` field order: levitating, water_breathing,
    // slow_falling, water_walking — `magic.rs:1619-1625`).
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    let (mut state, _) = cast_one(
        "Levitate",
        dur(1, 1, 1),
        &mut target,
        &mut dice,
    );
    state.cast_effects(
        &[
            entry("Slowfall", dur(1, 1, 1)),
            entry("WaterBreathing", dur(1, 1, 1)),
            entry("WaterWalking", dur(1, 1, 1)),
        ],
        1,
        &mut target,
        &mut dice,
    );
    let live = target.movement_effects();
    state.magic_round_with(1, &mut target, &mut dice);
    state.magic_round_with(1, &mut target, &mut dice);
    let after = target.movement_effects();
    rows.push(row(
        "E8-movement-flags",
        format!(
            "live=({levitating},{breathing},{falling},{walking}) after_expiry=({},{},{},{})",
            after.levitating, after.water_breathing, after.slow_falling, after.water_walking,
            levitating = live.levitating,
            breathing = live.water_breathing,
            falling = live.slow_falling,
            walking = live.water_walking,
        ),
        "live=(true,true,true,true) after_expiry=(false,false,false,false)".to_string(),
        "Levitate/Slowfall/WaterBreathing/WaterWalking Start/End (`Levitate.cs:60-81`, `Slowfall.cs:59-74`, `WaterBreathing.cs:58-79`, `WaterWalking.cs:91-98`)",
    ));

    // E9: cure disease.
    let mut target = applied_target();
    target.diseases = vec!["Disease-Legacy", "Disease-Other"];
    let mut dice = FixedDice::new(&[], 0);
    cast_one("Cure-Disease", duration_and_chance(50, 0, 1), &mut target, &mut dice);
    rows.push(row(
        "E9-cure-disease",
        format!("diseases=2->{}", target.diseases.len()),
        "diseases=2->0".to_string(),
        "CureAllDiseases (`CureDisease.cs:85`)",
    ));

    // E10: negative control — a failed OnCast roll drops the effect.
    let mut target = applied_target();
    target.poisons = vec!["Poison-Legacy", "Poison-Other"];
    let mut dice = FixedDice::new(&[], 50);
    let (_, report) = cast_one("Cure-Poison", duration_and_chance(50, 0, 1), &mut target, &mut dice);
    rows.push(row(
        "E10-chance-negcontrol",
        format!(
            "chance={} passed={} poisons=2->{}",
            report.applications[0].chance_value,
            report.applications[0].chance_passed,
            target.poisons.len()
        ),
        "chance=50 passed=false poisons=2->2".to_string(),
        "roll 50 is not < 50 ⇒ effect dropped (`EntityEffectManager.cs:541-555`)",
    ));

    // E11: teleport is a declared world request, not a silent move.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    cast_one("Teleport-Effect", EffectSettings::classic_default(), &mut target, &mut dice);
    rows.push(row(
        "E11-teleport-request",
        format!("teleport_requests=0->{}", target.request_count(Request::Teleport)),
        "teleport_requests=0->1".to_string(),
        "Teleport.cs:67 prompts the world; the neutral model emits a `Request`",
    ));

    // E12: the flags without an M19 consumer are set and declared as such —
    // `water_walking` is the exception: it is BOTH an entity flag and the 4th
    // `MovementEffects` field (M19 consumes it), which is exactly why the
    // movement tuple is four-wide (`magic.rs:1619-1625`).
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    let mut state = MagicState::default();
    let entries = [
        entry("WaterWalking", dur(1, 1, 1)),
        entry("FreeAction", dur(1, 1, 1)),
        entry("Shadow-Normal", dur(1, 1, 1)),
        entry("Invisibility-Normal", dur(1, 1, 1)),
        entry("Chameleon-Normal", dur(1, 1, 1)),
    ];
    let report = state.cast_effects(&entries, 1, &mut target, &mut dice);
    let movement = target.movement_effects();
    let raised: Vec<&str> = Flag::ALL
        .iter()
        .filter(|flag| target.has_flag(**flag))
        .map(|flag| flag.name())
        .collect();
    rows.push(row(
        "E12-no-consumer-flags",
        format!(
            "flags={raised:?} movement=({},{},{},{})",
            movement.levitating,
            movement.water_breathing,
            movement.slow_falling,
            movement.water_walking
        ),
        "flags=[\"water_walking\", \"free_action\"] movement=(false,false,false,true)".to_string(),
        format!(
            "entity flags ≠ M19 movement except water_walking (flag + `PlayerBody.water_walking`); the two concealment families carry no entity flag ({} state changes)",
            report.state_changes.len()
        ),
    ));

    // E13: unknown classic keys are reported, never silently skipped.
    let unimplemented = ClassicEffect {
        effect_type: 9,
        sub_type: 9,
        duration: [1, 1, 1],
        chance: [1, 1, 1],
        magnitude: [0; 5],
    };
    let empty = ClassicEffect {
        effect_type: -1,
        sub_type: -1,
        duration: [0, 0, 0],
        chance: [0, 0, 0],
        magnitude: [0; 5],
    };
    let mut unresolved = Vec::new();
    let resolved = resolve_classic_effects(&[empty, unimplemented], &mut unresolved);
    rows.push(row(
        "E13-unknown-key",
        format!(
            "resolved={} unresolved={unresolved:?}",
            resolved.len()
        ),
        "resolved=0 unresolved=[2313]".to_string(),
        "(9,9) = 9·256+9 = 2313 has no template (§6 N-1); empty slot (-1) is padding",
    ));

    // E14: two instances hold the flag until the last one ends.
    let mut target = applied_target();
    let mut dice = FixedDice::new(&[], 0);
    let mut state = MagicState::default();
    state.cast_effects(
        &[
            entry("Levitate", dur(1, 1, 1)),
            entry("Levitate", dur(1, 2, 1)),
        ],
        1,
        &mut target,
        &mut dice,
    );
    let mut trace = vec![target.has_flag(Flag::Levitating)];
    for _ in 0..3 {
        state.magic_round_with(1, &mut target, &mut dice);
        trace.push(target.has_flag(Flag::Levitating));
    }
    rows.push(row(
        "E14-two-instances",
        format!("flag_trace={trace:?} active={}", state.active().len()),
        "flag_trace=[true, true, true, false] active=0".to_string(),
        "flag stays while one instance lives; End of the last clears it",
    ));

    rows
}

// ── PROBE-SPELLMAKER-001 · blocks A-D ──────────────────────────────────────

/// Block A: displayed cost (spellmaker) vs. charged cost (book) over 25
/// deterministic bundles (5 targets × 5 skills) — `mismatch=0` (spec §9 A).
fn spellmaker_rows_a() -> Vec<Row> {
    let mut mismatch = 0usize;
    let mut checked = 0usize;
    for target in [
        TargetType::CasterOnly,
        TargetType::ByTouch,
        TargetType::SingleTargetAtRange,
        TargetType::AreaAroundCaster,
        TargetType::AreaAtRange,
    ] {
        for skill in [0, 25, 50, 75, 100] {
            let entries = [demo_entries()[0], demo_entries()[1]];
            let mut maker = SpellMaker::new(1, target);
            maker.add_effect(entries[0]).expect("free slot");
            maker.add_effect(entries[1]).expect("free slot");
            let displayed = maker.displayed_cost(&|_| skill);
            let charged = calculate_total_effect_costs(&entries, target, false, &|_| skill);
            checked += 1;
            if displayed != charged {
                mismatch += 1;
            }
        }
    }
    vec![row(
        "A-mismatch",
        format!("bundles={checked} mismatch={mismatch}"),
        "bundles=25 mismatch=0".to_string(),
        "displayed cost == charged cost (§5 spellmaker, §10.4)",
    )]
}

/// Block B: spellmaker limits (spec §9 B).
fn spellmaker_rows_b() -> Vec<Row> {
    let mut maker = SpellMaker::default();
    let entry = demo_entries()[0];
    maker.add_effect(entry).expect("slot 1");
    maker.add_effect(entry).expect("slot 2");
    maker.add_effect(entry).expect("slot 3");
    let fourth = maker.add_effect(entry);
    let fourth_rejected = fourth == Err(SpellmakerError::NoFreeSlot);
    let row_fourth = row(
        "B-fourth",
        format!("fourth_rejected={fourth_rejected} entries={}", maker.entries().len()),
        "fourth_rejected=true entries=3".to_string(),
        "only 3 effects per spell (RSC 1707, `:706-722`)",
    );

    // `points < 5 ⇒ 5`: skill 100 on Damage-Health ⇒ 1 point per effect.
    let low = calculate_total_effect_costs(&[demo_entries()[0]], TargetType::CasterOnly, false, &|_| 100);
    let row_floor = row(
        "B-floor",
        format!("points={}", low.spell_points),
        "points=5".to_string(),
        "floor of 5 points (`FormulaHelper.cs:2193`)",
    );

    // `minimumCastingCost ⇒ 5` exact (even when it would be 13).
    let minimum = calculate_total_effect_costs(&[demo_entries()[0]], TargetType::CasterOnly, true, &|_| 0);
    let row_minimum = row(
        "B-minimum",
        format!("gold={} points={}", minimum.gold, minimum.spell_points),
        "gold=48 points=5".to_string(),
        "minimumCastingCost fixes points = 5 (`:2218-2224`)",
    );

    vec![row_fourth, row_floor, row_minimum]
}

/// Block C: book purchase price (spec §9 C, §5).
fn spellmaker_rows_c() -> Vec<Row> {
    vec![row(
        "C-price",
        format!(
            "(46→{}) (46festival→{}) (0festival→{})",
            purchase_price(46, false),
            purchase_price(46, true),
            purchase_price(0, true)
        ),
        "(46→184) (46festival→92) (0festival→1)".to_string(),
        "sp·4; half at Witches Festival with minimum 1 (`:518-532`)",
    )]
}

/// Block D: casting animation (spec §9 D, §4).
fn spellmaker_rows_d() -> Vec<Row> {
    let release = cast_release_seconds();
    let full = cast_full_seconds();
    let frames = CAST_FRAME_INDICES.len();
    vec![
        row(
            "D-release",
            format!("frame={} seconds={release:.2}", CAST_RELEASE_FRAME),
            "frame=5 seconds=0.20".to_string(),
            "releaseFrame 5 · 0.04 s (`FPSSpellCasting.cs:48-52`)",
        ),
        row(
            "D-frames",
            format!("frames={frames} seconds={full:.2}"),
            "frames=7 seconds=0.28".to_string(),
            "frameIndices {0,1,2,3,4,5,0}",
        ),
    ]
}

// ── runners ─────────────────────────────────────────────────────────────────

/// `--probe MAGIC-001` — parse + classic cost + casting (spec §9).
/// `EXIT=2` if ARENA2 or `SPELLS.STD` is not accessible (blocks A/B = census).
pub fn run_magic_001() -> i32 {
    println!(
        "PROBE-MAGIC-001 - M23 magic (REQ-L99-056 · CR-a0b60b1c · spec §9 A-D)"
    );
    let path = match crate::mode_a_arena2_path("SPELLS.STD") {
        Ok(path) => path,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-MAGIC-001 NO DATA - ARENA2 not accessible (A/B = census; never a PASS without retail)"
            );
            return 2;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("ARENA2 not accessible: reading {path:?}: {err}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-MAGIC-001 NO DATA - SPELLS.STD unreadable (never a PASS without retail)"
            );
            return 2;
        }
    };
    println!("[OK] mode_a SPELLS.STD: {} ({} B)", path.display(), bytes.len());

    let records = parse_spells_std(&bytes);
    let censo = census(&records);
    println!(
        "records={} internal={} zero_cost={} max_cost={} unique_index={} offered={}",
        censo.records, censo.internal, censo.zero_cost, censo.max_cost,
        censo.unique_indexes, censo.offered
    );

    let a_rows = block_a_rows(&bytes);
    let a_ok = print_rows("A · census §1.3 + goldens A1-A8", &a_rows);
    let b_rows = block_b_rows(&records);
    let b_ok = print_rows("B · §3.2 over the 89 records", &b_rows);
    let c_rows = block_c_rows();
    let c_ok = print_rows("C · §3.1 synthetic bundles D1-D12", &c_rows);
    let d_rows = block_d_rows();
    let d_ok = print_rows("D · demo casting (touch/range/area)", &d_rows);

    let checks = a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len();
    let code = i32::from(!(a_ok && b_ok && c_ok && d_ok));
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-MAGIC-001 PASS - A∧B∧C∧D · census 89/12/8/2950/88/76 · calc_cost_ok=88/89 upstream_skip=1 (record 71 !Indulcet, 0 divisor equivalent to C#) · E1=28 E2=70 E3=33 E4=360 E5=3600 · D1-D12 exact · demo ×3 measured"
        );
    } else {
        println!("VERDICT: PROBE-MAGIC-001 FAIL - see [FAIL] rows (checks={checks})");
    }
    code
}

/// `--probe EFFECT-001` — effect model (spec §9 A-D). 100% synthetic
/// (0 retail data ⇒ never `EXIT=2`).
pub fn run_effect_001() -> i32 {
    println!(
        "PROBE-EFFECT-001 - M23 effect model (REQ-L99-056 · CR-a0b60b1c · spec §9 A-D)"
    );
    let a_rows = effect_rows_a();
    let a_ok = print_rows("A · goldens B1-B6 + C1-C6 (§2.1)", &a_rows);
    let b_rows = effect_rows_b();
    let b_ok = print_rows("B · runtime registration", &b_rows);
    let c_rows = effect_rows_c();
    let c_ok = print_rows("C · instance isolation", &c_rows);
    let d_rows = effect_rows_d();
    let d_ok = print_rows("D · key map (digest ×2)", &d_rows);
    let e_rows = effect_rows_e();
    let e_ok = print_rows("E · applied mutations (before/after, one per family)", &e_rows);
    let f_rows = effect_rows_f();
    let f_ok = print_rows(
        "F · per-template + per-payload evidence, 165/165 keys (contract §5)",
        &f_rows,
    );

    let checks = a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len() + e_rows.len() + f_rows.len();
    let passed = a_rows
        .iter()
        .chain(&b_rows)
        .chain(&c_rows)
        .chain(&d_rows)
        .chain(&e_rows)
        .chain(&f_rows)
        .filter(|row| row.ok)
        .count();
    let failed = checks - passed;
    let code = i32::from(!(a_ok && b_ok && c_ok && d_ok && e_ok && f_ok));
    println!("checks={checks} passed={passed} failed={failed}");
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-EFFECT-001 PASS - A∧B∧C∧D∧E∧F · B1-B6 + C1-C6 (C6 without clamp) · 165 keys / 153 DFU templates / 0 collisions / 4 variant families (22 typed variants) · isolated instances · key map + digest ×2 · applied health/fatigue/spell points/attribute/resistance/flags with expiry and chance controls · F: one row per key (applied>0, duration⇒round, permanent⇒alive, revert) + per-payload F-p/F-e/F-neg/F-mut with frozen census"
        );
    } else {
        println!("VERDICT: PROBE-EFFECT-001 FAIL - see [FAIL] rows");
    }
    code
}

/// `--probe SPELLMAKER-001` — creation (spec §9 A-D). 100% synthetic
/// (0 retail data ⇒ never `EXIT=2`).
pub fn run_spellmaker_001() -> i32 {
    println!(
        "PROBE-SPELLMAKER-001 - M23 spellmaker (REQ-L99-056 · CR-a0b60b1c · spec §9 A-D)"
    );
    let a_rows = spellmaker_rows_a();
    let a_ok = print_rows("A · displayed vs. charged cost (25 bundles)", &a_rows);
    let b_rows = spellmaker_rows_b();
    let b_ok = print_rows("B · limits (4th / floor 5 / minimum)", &b_rows);
    let c_rows = spellmaker_rows_c();
    let c_ok = print_rows("C · purchase price", &c_rows);
    let d_rows = spellmaker_rows_d();
    let d_ok = print_rows("D · casting animation", &d_rows);

    let checks = a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len();
    let passed = a_rows
        .iter()
        .chain(&b_rows)
        .chain(&c_rows)
        .chain(&d_rows)
        .filter(|row| row.ok)
        .count();
    let failed = checks - passed;
    let code = i32::from(!(a_ok && b_ok && c_ok && d_ok));
    println!("checks={checks} passed={passed} failed={failed}");
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-SPELLMAKER-001 PASS - A∧B∧C∧D · 25/25 without mismatch · 4th rejected · floor 5 · (46→184)(46f→92)(0f→1) · 0.20 s / 0.28 s"
        );
    } else {
        println!("VERDICT: PROBE-SPELLMAKER-001 FAIL - see [FAIL] rows");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_c_rows_are_12_and_all_pass() {
        let rows = block_c_rows();
        assert_eq!(rows.len(), 12, "spec §9 C: D1-D12");
        for row in &rows {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    #[test]
    fn block_d_rows_are_4_and_all_pass() {
        let rows = block_d_rows();
        assert_eq!(rows.len(), 4, "3 targets + digest");
        for row in &rows {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    #[test]
    fn demo_trace_is_deterministic_across_runs() {
        assert_eq!(demo_trace(), demo_trace(), "canonical trace is deterministic");
        assert_eq!(fnv1a64(demo_trace().as_bytes()), fnv1a64(demo_trace().as_bytes()));
    }

    #[test]
    fn effect_rows_are_all_present_and_pass() {
        assert_eq!(effect_rows_a().len(), 12, "B1-B5 + B6 + C1-C6 = 12");
        assert_eq!(effect_rows_b().len(), 5, "B-registered + census + collisions + variants + lookup");
        assert_eq!(effect_rows_c().len(), 2);
        assert_eq!(effect_rows_d().len(), 2);
        assert_eq!(effect_rows_e().len(), 14, "E1-E14 applied mutations");
        assert_eq!(
            effect_rows_f().len(),
            TEMPLATE_COUNT + LEAF_PAYLOAD_COUNT + F_TRIGGER_TEMPLATE_COUNT + 6,
            "F: 165 key rows + F-all-templates-act + F-census + one F-p per leaf + one F-e per trigger template + F-neg x2 + F-mut + F-payload-census"
        );
        for row in effect_rows_a()
            .into_iter()
            .chain(effect_rows_b())
            .chain(effect_rows_c())
            .chain(effect_rows_d())
            .chain(effect_rows_e())
            .chain(effect_rows_f())
        {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    /// The negative control of contract §5.8, as a test: a template that
    /// emits a visible `Request` passes the template-level evidence even
    /// though its per-round payload rolls magnitude 0 (no `SupportMagnitude`)
    /// and changes nothing — the exact blindness that hid `Passive-Specials`
    /// inert regeneration (QA MAJOR-2). The per-payload detector must flag
    /// the dead leaf RED; without it the probe cannot distinguish "applied"
    /// from "never applied".
    #[test]
    fn the_per_payload_detector_flags_a_dead_payload_the_template_evidence_cannot_see() {
        const DEAD: EffectProperties = EffectProperties::new(
            "F-neg-dead-injection",
            0,
            MagicSkill::None,
            &[
                Payload::Request(Request::Identify),
                Payload::ResourcePerRound {
                    resource: Resource::Fatigue,
                    sign: -1,
                },
            ],
        );
        assert!(
            template_evidence(&DEAD).visible(),
            "the template-level check stays blind to the dead payload (the MAJOR-2 gap this control reproduces)"
        );
        let leaves = flatten_leaves(DEAD.payload);
        assert_eq!(leaves.len(), 2, "one leaf per declared payload");
        let request = leaf_evidence(&DEAD, &leaves[0]);
        assert_eq!(request.class, LeafClass::Applied);
        assert!(
            request.ok,
            "the live Request payload passes: got={} want={}",
            request.got,
            request.want
        );
        let per_round = leaf_evidence(&DEAD, &leaves[1]);
        assert_eq!(per_round.class, LeafClass::Applied);
        assert!(
            !per_round.ok,
            "the dead per-round payload (expectation computes to zero) must be flagged RED: got={} want={}",
            per_round.got,
            per_round.want
        );
    }

    #[test]
    fn spellmaker_rows_are_all_present_and_pass() {
        assert_eq!(spellmaker_rows_a().len(), 1);
        assert_eq!(spellmaker_rows_b().len(), 3);
        assert_eq!(spellmaker_rows_c().len(), 1);
        assert_eq!(spellmaker_rows_d().len(), 2);
        for row in spellmaker_rows_a()
            .into_iter()
            .chain(spellmaker_rows_b())
            .chain(spellmaker_rows_c())
            .chain(spellmaker_rows_d())
        {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    #[test]
    fn effect_map_digest_is_stable_and_excludes_key_zero() {
        let map = effect_map();
        // 165 rows: 91 declare a non-zero `ClassicKey` (the 82 DFU declarations
        // expand through the typed variant families), the rest keep key 0.
        assert_eq!(map.len(), 91, "one entry per distinct non-zero ClassicKey");
        assert!(!map.contains_key(&0), "ClassicKey 0 is not mapped (§6 N-2)");
    }
}
