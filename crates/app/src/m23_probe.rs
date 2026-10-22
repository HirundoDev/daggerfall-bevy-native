//! M23 — anti-screen probes of **magic** (REQ-L99-056): `PROBE-MAGIC-001`
//! (parse + classic cost + casting), `PROBE-EFFECT-001` (effect model) and
//! `PROBE-SPELLMAKER-001` (creation).
//!
//! Contract: `docs/sintergic/game/programming/m23-magic-spec-CR-5243af63.md`
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
//! block B runtime registration (`templates_registered=22` M23 ·
//! `dfu_census=153` §6 N-1, collisions 0, variants >1); block C instance
//! isolation; block D canonical digest `(classicKey → effectKey)` ×2.
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

use std::collections::BTreeMap;

use daggerfall_behavior::magic::{
    apply_free_action_patch, calculate_total_effect_costs, cast_full_seconds,
    cast_release_seconds, casting_cost_enchanting, chance_value, classic_effect_entry,
    component_cost, effect_by_key, effect_cost, enchant_pt_cost, make_effect_costs,
    purchase_price, roll_chance, rounds, ClassicEffect, EffectEntry, EffectProperties,
    EffectSettings, MagicSkill, MagicState, REFERENCE_EFFECTS, SETTINGS_TYPES, SpellMaker,
    SpellmakerError, TargetType, CAST_FRAME_INDICES, CAST_RELEASE_FRAME,
};
use daggerfall_parser_rs::spells_std::{census, parse_spells_std, SPELL_RECORD_SIZE};

// ── evidence rows (M19/M20/M21/M22 pattern) ──────────────────────────────

/// Row printed by the probe: `[OK] <id> - <detail>` or
/// `FAIL <id> got=<v> want=<v>` (verbatim from spec §9).
#[derive(Debug, Clone)]
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want`.
fn row(id: &'static str, got: String, want: String, detail: impl Into<String>) -> Row {
    Row {
        id,
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
            println!("[OK] {} - {}", row.id, row.detail);
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
    const GOLD8: EffectProperties = EffectProperties {
        key: "D11-Gold-8",
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

/// Block B: runtime registration (spec §9 B; NEGATIVE N-1: only engine +
/// 17 reference effects + 5 variants ⇒ 22 templates, not 153).
fn effect_rows_b() -> Vec<Row> {
    let mut registered = 0usize;
    let mut collisions = 0usize;
    let mut variants = 0usize;
    let mut keys: BTreeMap<i32, &str> = BTreeMap::new();
    for props in REFERENCE_EFFECTS {
        registered += 1;
        if props.variant_count > 1 {
            variants += 1;
        }
        // `MapClassicKey`: key 0 is not mapped (spec §6 N-2/N-1).
        if props.classic_key != 0 && keys.insert(props.classic_key, props.key).is_some() {
            collisions += 1;
        }
    }
    println!(
        "templates_registered={registered} dfu_census=153 (NEGATIVE N-1: M23 registers the engine + 17 reference + 5 variants)"
    );
    vec![
        row(
            "B-registered",
            format!("templates_registered={registered}"),
            "templates_registered=22".to_string(),
            "17 reference effects + 5 variants (§6 N-1)",
        ),
        row(
            "B-collisions",
            format!("classic_key_collisions={collisions}"),
            "classic_key_collisions=0".to_string(),
            "no collision in the runtime registration",
        ),
        row(
            "B-variants",
            format!("variants_gt1={variants}"),
            "variants_gt1=5".to_string(),
            "only ElementalResistance declares variantCount 5",
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

/// Block D: canonical digest of the `(classicKey → effectKey)` map ×2
/// (spec §9 D; NEGATIVE N-2: the map is built at runtime by
/// inspecting the registry).
fn effect_map() -> BTreeMap<i32, &'static str> {
    let mut map = BTreeMap::new();
    for props in REFERENCE_EFFECTS {
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
    vec![
        row(
            "D-map-entries",
            format!("map_entries={}", map.len()),
            "map_entries=21".to_string(),
            "22 templates − 1 without classicKey (Heal-SpellPoints)",
        ),
        row(
            "D-digest",
            format!("digest={first}"),
            format!("digest={second}"),
            "sorted (classicKey → effectKey) map, digest ×2 identical",
        ),
    ]
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
            "VERDICT: PROBE-EFFECT-001 PASS - A∧B∧C∧D · B1-B6 + C1-C6 (C6 without clamp) · 22 templates / 0 collisions / 5 variants · isolated instances · digest ×2"
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
        assert_eq!(effect_rows_b().len(), 3);
        assert_eq!(effect_rows_c().len(), 2);
        assert_eq!(effect_rows_d().len(), 2);
        for row in effect_rows_a()
            .into_iter()
            .chain(effect_rows_b())
            .chain(effect_rows_c())
            .chain(effect_rows_d())
        {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
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
        assert_eq!(map.len(), 21, "22 templates minus Heal-SpellPoints (key 0)");
        assert!(!map.contains_key(&0), "ClassicKey 0 is not mapped (§6 N-2)");
    }
}
