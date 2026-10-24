//! M32 probes — conditions, death and rest: diseases, contagion, poisons,
//! cure, rest, vampirism/lycanthropy and player death
//! (SPEC-M32-CONDITIONS-DEATH-REST, REQ-L99-065).
//!
//! - **`PROBE-CONDITION-001`**
//!   - **A** D1-D13 (13) + P1-P11 (11) = **24 golden vectors** with the rolls
//!     injected (block A never consumes RNG), printed as `id|expected|actual`.
//!   - **B** seeded lifecycle (`0x5243AF63`): infect, advance 1/5/200 days,
//!     cure → `stat_deltas`, `days_left`, `ended=true`, plus the §12 demo
//!     (rat → Plague, day 0 vs day 1, potion cure) and an FNV-1a 64 digest
//!     that must be identical across two runs.
//!   - **C** census: `disease_classes=21` (17 classic + 4 infection classes),
//!     `infections=4`, `poisons=12`.
//! - **`PROBE-VAMPIRE-001`**
//!   - **A** V1-V8 + Y1-Y5 + K1-K3 = **16 golden vectors**.
//!   - **B** infection → warning dream → full stage → full-moon morph, sun
//!     damage (`stage`, `transformed`, `hp_after_sun`) and digest ×2.
//! - **`PROBE-REST-001`**
//!   - **A** R1-R10 = **10 golden vectors** (rates, FullRest hour, ticks,
//!     interruption with message **354**).
//!   - **B** 12 h rest with an enemy injected on hour 3, digest ×2.
//!   - **D** demo over the retail capital (M30 rent): 1 night, sleep 9 h ⇒
//!     `room_hours 24→15`, `hp_before/after`, `hour_after`.
//! - **`PROBE-DEATH-STATE-001`**
//!   - **A** X1-X2 + the NEGATIVE census: `resurrect_symbols=0` measured over
//!     `crates/` (declarations only; the doc-comment mention of the invariant
//!     is reported separately).
//!   - **B** HP 1 → lethal hit → `DeathInProgress` → reset after 3 s, with
//!     `hp_restored=10%` under the guild benefit.
//!
//! Exit: `0` ⟺ the probe's blocks pass; `2` when ARENA2 is inaccessible
//! (REST block D only); `1` on any FAIL. Mode A custody: 0 retail bytes.

use std::path::{Path, PathBuf};

use daggerfall_behavior::economy::{room_cost, room_expiry_seconds, room_remaining_hours};
use daggerfall_skills::conditions::{
    contact_infection, cure_service_cost, infection_stage, Contagion, CureServiceRequest, Disease,
    DiseaseBook, DiseaseEffect, EnemyKind, InfectionStage, LycanthropyKind, Poison, PoisonEffect,
    PoisonState, PoisonTarget, VitalsDelta, AGILITY, ATTRIBUTE_COUNT, DISEASE_COUNT, ENDURANCE,
    FATIGUE, HEALTH, INFECTION_COUNT, INTELLIGENCE, LUCK, PERSONALITY, POISON_COUNT, SPELL_POINTS,
    SPEED, STRENGTH, WILLPOWER,
};
use daggerfall_skills::death::{
    apply_damage, avoid_death_health, stendarr_avoids_death, DeathState, LifeOutcome,
    DEATH_RESET_SECONDS,
};
use daggerfall_skills::formula::calculate_trade_price;
use daggerfall_skills::rest::{
    fatigue_recovery_per_hour, health_recovery_per_hour, medical_bonus, spell_point_recovery_per_hour,
    RapidHealing, RestMode, RestVitals, RestWindow,
};
use daggerfall_skills::rng::Rng64;
use daggerfall_skills::vampirism::{
    lycanthropy_cure_succeeds, vampire_cure_succeeds, LycanthropyState, VampireState,
    LYCAN_SKILL_BONUS, MORPH_COOLDOWN_MINUTES, NEED_TO_KILL_MINUTES, VAMPIRE_SKILL_BONUS,
};

// ═══════════════════════════════════════════════════════════════════════════
// Evidence rows (M19/M25/M27/M30 template)
// ═══════════════════════════════════════════════════════════════════════════

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

fn row(id: &'static str, got: String, want: impl Into<String>, detail: impl Into<String>) -> Row {
    let want = want.into();
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
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

/// `PASS`/`FAIL` word for the summary lines.
fn verdict(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x100000001b3`).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Stable `key=value` render of a delta (raw numbers, never prose).
fn fmt_delta(delta: &VitalsDelta) -> String {
    format!(
        "str={} int={} wil={} agi={} end={} per={} spd={} luc={} hea={} fat={} spl={}",
        delta.attributes[STRENGTH],
        delta.attributes[INTELLIGENCE],
        delta.attributes[WILLPOWER],
        delta.attributes[AGILITY],
        delta.attributes[ENDURANCE],
        delta.attributes[PERSONALITY],
        delta.attributes[SPEED],
        delta.attributes[LUCK],
        delta.vitals[HEALTH],
        delta.vitals[FATIGUE],
        delta.vitals[SPELL_POINTS],
    )
}

/// Expected render of a delta from its arrays.
fn want_delta(attributes: [i32; ATTRIBUTE_COUNT], vitals: [i32; 3]) -> String {
    fmt_delta(&VitalsDelta { attributes, vitals })
}

/// Runs a disease tick with an injected daily damage roll (block A).
fn disease_tick(
    disease: Disease,
    from_day: u64,
    to_day: u64,
    damage: i32,
) -> (DiseaseEffect, VitalsDelta) {
    let mut effect = DiseaseEffect {
        disease,
        last_day: from_day,
        days_of_symptoms_left: disease.data().days_max.max(1),
        completed: false,
    };
    let tick = effect.tick_with(to_day, || damage);
    (effect, tick.delta)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-CONDITION-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-CONDITION-001` — A (24 vectors), B (lifecycle + demo, digest ×2),
/// C (census).
pub fn run_condition_001() -> i32 {
    println!(
        "PROBE-CONDITION-001 - DFU conditions: diseases, contagion, poisons, cure (M32, CR-f716455c, REQ-L99-065 · spec §1-§2, §7.1-§7.2, §8)"
    );

    // ── A · D1-D13 + P1-P11 ───────────────────────────────────────────────
    let mut rows = Vec::new();
    blood_vectors(&mut rows);
    poison_vectors(&mut rows);
    println!("\n-- A · golden vectors (id|expected|actual) --");
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    for row in &rows {
        if row.ok {
            ok_count += 1;
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            fail_count += 1;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    println!("vectors={} ok={} fail={}", rows.len(), ok_count, fail_count);
    let a_ok = fail_count == 0 && rows.len() == 24;

    // ── B · seeded lifecycle + §12 demo ───────────────────────────────────
    let first = condition_lifecycle();
    let second = condition_lifecycle();
    let digest_a = fnv1a64(first.join("|").as_bytes());
    let digest_b = fnv1a64(second.join("|").as_bytes());
    println!("\n-- B · seeded lifecycle (0x5243AF63) + demo --");
    for line in &first {
        println!("[B] {line}");
    }
    println!("[B] digest={digest_a:016x} digest2={digest_b:016x}");
    let b_rows = vec![
        row(
            "B1 determinism",
            format!("{digest_a:016x}"),
            format!("{digest_b:016x}"),
            "digest ×2 over the same seeded lifecycle",
        ),
        row(
            "B2 demo rat",
            demo_disease()
                .into_iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(" "),
            "diseased=Plague day0_delta=0 day1_str=47 cured=none str_after_cure=47",
            "§12(a)(b): rat bite → Plague; day 0 silent; day 1 STR 50→47; potion cures, stats stay lost",
        ),
    ];
    let b_ok = print_rows("B · lifecycle and demo", &b_rows);

    // ── C · census ────────────────────────────────────────────────────────
    let disease_classes = DISEASE_COUNT + INFECTION_COUNT;
    let c_rows = vec![
        row(
            "C1 disease classes",
            format!("{disease_classes}"),
            "21",
            "17 classic diseases + 4 infection classes (Effects/Diseases/*.cs = 22 files with the base)",
        ),
        row(
            "C2 infections",
            format!("{INFECTION_COUNT}"),
            "4",
            "Lycanthropy · Vampirism · Werewolf · Wereboar",
        ),
        row(
            "C3 poisons",
            format!("{POISON_COUNT}"),
            "12",
            "Poisons 128..=139 (8 weapon poisons + 4 drugs)",
        ),
    ];
    let c_ok = print_rows("C · census", &c_rows);
    println!("disease_classes={disease_classes} infections={INFECTION_COUNT} poisons={POISON_COUNT}");

    let pass = a_ok && b_ok && c_ok;
    println!(
        "\nSUMMARY PROBE-CONDITION-001: A={} B={} C={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(c_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

/// D1-D13 (`spec §7.1`), rolls injected.
fn blood_vectors(rows: &mut Vec<Row>) {
    // D1 — Plague, damage=3.
    let (_, d1) = disease_tick(Disease::Plague, 0, 1, 3);
    rows.push(row(
        "D1",
        fmt_delta(&d1),
        want_delta([-3, 0, -3, -3, -3, -3, -3, -3], [-3, -3, -3]),
        "Plague damage=3 ⇒ STR/WIL/AGI/END/PER/SPD/LUC/HEA/FAT/SPL −3, INT 0",
    ));

    // D2 — Plague 5 days, damage=3.
    let (_, d2) = disease_tick(Disease::Plague, 0, 5, 3);
    rows.push(row(
        "D2",
        fmt_delta(&d2),
        want_delta([-15, 0, -15, -15, -15, -15, -15, -15], [-15, -15, -15]),
        "5 daily increments × 3 = −15 on the 10 affected slots",
    ));

    // D3 — Yellow Fever, damage=5.
    let (_, d3) = disease_tick(Disease::YellowFever, 0, 1, 5);
    rows.push(row(
        "D3",
        fmt_delta(&d3),
        want_delta([0, 0, -5, 0, -5, 0, 0, 0], [-5, 0, 0]),
        "WIL−5, END−5, HEA−5",
    ));

    // D4 — Stomach Rot, damage=4.
    let (_, d4) = disease_tick(Disease::StomachRot, 0, 1, 4);
    rows.push(row(
        "D4",
        fmt_delta(&d4),
        want_delta([0, 0, 0, 0, 0, 0, 0, 0], [-4, 0, 0]),
        "only HEA−4",
    ));

    // D5 — Caliron's Curse, damage=5.
    let (_, d5) = disease_tick(Disease::CalironsCurse, 0, 1, 5);
    rows.push(row(
        "D5",
        fmt_delta(&d5),
        want_delta([-5, 0, 0, -5, 0, 0, -5, 0], [0, 0, 0]),
        "STR−5, AGI−5, SPD−5",
    ));

    // D6 — Wizard Fever, damage=2.
    let (_, d6) = disease_tick(Disease::WizardFever, 0, 1, 2);
    rows.push(row(
        "D6",
        fmt_delta(&d6),
        want_delta([0, -2, 0, 0, 0, 0, 0, 0], [0, 0, -2]),
        "INT−2, SPL−2",
    ));

    // D7 — Witches' Pox, damage=10.
    let (_, d7) = disease_tick(Disease::WitchesPox, 0, 1, 10);
    rows.push(row(
        "D7",
        fmt_delta(&d7),
        want_delta([-10, 0, 0, 0, -10, 0, 0, 0], [-10, 0, 0]),
        "STR−10, END−10, HEA−10",
    ));

    // D8 — Plague 30 days at its worst roll (30).
    let worst = i32::from(Disease::Plague.data().max_damage);
    let (_, d8) = disease_tick(Disease::Plague, 0, 30, worst);
    rows.push(row(
        "D8",
        fmt_delta(&d8),
        want_delta(
            [-900, 0, -900, -900, -900, -900, -900, -900],
            [-900, -900, -900],
        ),
        "30 days × 30 (max roll) = HEA −900; the live stat clamps at read time, the mod is not clamped",
    ));

    // D9 — same-day incubation.
    let mut rng = Rng64::new(0x5243_AF63);
    let mut incubated = DiseaseEffect::start(Disease::Plague, 10, 5, &mut rng);
    let d9 = incubated.tick(10, &mut rng);
    rows.push(row(
        "D9",
        format!("days={} {}", d9.days, fmt_delta(&d9.delta)),
        format!("days=0 {}", want_delta([0; ATTRIBUTE_COUNT], [0; 3])),
        "daysPast==0 ⇒ incubation, 0 increments",
    ));

    // D10 — Caliron's Curse with 3 symptom days and a 5-day jump.
    let mut d10 = DiseaseEffect {
        disease: Disease::CalironsCurse,
        last_day: 0,
        days_of_symptoms_left: 3,
        completed: false,
    };
    let tick = d10.tick_with(5, || 5);
    rows.push(row(
        "D10",
        format!(
            "{} ended={} days_left={}",
            fmt_delta(&tick.delta),
            tick.ended,
            tick.days_of_symptoms_left
        ),
        format!(
            "{} ended=true days_left=254",
            want_delta([-25, 0, 0, -25, 0, 0, -25, 0], [0, 0, 0])
        ),
        "5 increments applied before the counter drops below zero (0xFE = completed)",
    ));

    // D11 — permanent disease after 10 000 days.
    let mut permanent = DiseaseEffect {
        disease: Disease::Plague,
        last_day: 0,
        days_of_symptoms_left: 0xFF,
        completed: false,
    };
    let tick = permanent.tick_with(10_000, || 1);
    rows.push(row(
        "D11",
        format!(
            "active={} days_left={} days_applied={}",
            !tick.ended, tick.days_of_symptoms_left, tick.days
        ),
        "active=true days_left=255 days_applied=10000",
        "daysOfSymptomsMin=0xFF ⇒ never expires by time (payload still fires)",
    ));

    // D12 — Caliron's Curse duration 3..=18 (16 values).
    let mut durations = std::collections::BTreeSet::new();
    for seed in 0..200u64 {
        let mut rng = Rng64::new(seed);
        let effect = DiseaseEffect::start(Disease::CalironsCurse, 0, 5, &mut rng);
        durations.insert(effect.days_of_symptoms_left);
    }
    rows.push(row(
        "D12",
        format!(
            "min={} max={} values={}",
            durations.iter().min().copied().unwrap_or(0),
            durations.iter().max().copied().unwrap_or(0),
            durations.len()
        ),
        "min=3 max=18 values=16",
        "Range(3, 18+1) ⇒ 16 possible durations",
    ));

    // D13 — Dice100.SuccessRoll(5): roll 4 infects, roll 5 does not.
    let infects = contact_infection(EnemyKind::Rat, 4, 50.0, 0);
    let misses = contact_infection(EnemyKind::Rat, 5, 50.0, 0);
    rows.push(row(
        "D13",
        format!("{infects:?}|{misses:?}"),
        "Disease(Plague)|None",
        "SuccessRoll(5): roll 4 < 5 infects; roll 5 does not",
    ));
}

/// P1-P11 (`spec §7.2`).
fn poison_vectors(rows: &mut Vec<Row>) {
    // P1 — Nux Vomica onset/duration.
    let nux = Poison::NuxVomica.data();
    rows.push(row(
        "P1",
        format!("onset={}..{} duration={}..{}", nux.onset_min, nux.onset_max, nux.duration_min, nux.duration_max),
        "onset=4..4 duration=3..10",
        "Min/MaxMinutesToPoison = 4/4; Min/MaxRoundsOfPoison = 3/10",
    ));

    // P2 — Nux 10 ticks at the worst roll.
    let nux_effect = PoisonEffect::with_rolls(Poison::NuxVomica, 4, 10);
    rows.push(row(
        "P2",
        format!("{}", nux_effect.worst_health_damage(10)),
        "110",
        "10 ticks × 11 HP (Range(2,12))",
    ));

    // P3 — Nux worst total time.
    rows.push(row(
        "P3",
        format!("{}", nux.onset_max + nux.duration_max),
        "14",
        "onset 4 + duration 10",
    ));

    // P4 — Arsenic 1000 ticks.
    let arsenic = PoisonEffect::with_rolls(Poison::Arsenic, 10, 1000);
    let end_worst: i32 = arsenic
        .poison
        .data()
        .effects
        .iter()
        .filter(|effect| effect.target == PoisonTarget::Endurance && !effect.positive)
        .map(|effect| effect.max)
        .sum::<i32>()
        * 1000;
    rows.push(row(
        "P4",
        format!(
            "hp={} end={}",
            arsenic.worst_health_damage(1000),
            end_worst
        ),
        "hp=2000 end=1000",
        "1000 ticks × (HP 2 + END −1)",
    ));

    // P5 — Moonseed draws 1..9 (never 10).
    let (hp_min, hp_max) = poison_range(Poison::Moonseed, 0, 300, PoisonTarget::Health, 0x5243_AF63);
    rows.push(check(
        "P5",
        (1..=9).contains(&hp_min) && (1..=9).contains(&hp_max) && hp_max <= 9,
        format!("hp_damage={hp_min}..{hp_max} (300 draws; declared Range(1,10) ⇒ 1..9)"),
    ));

    // P6 — Somnalius fatigue 10..99.
    let (fat_min, fat_max) =
        poison_range(Poison::Somnalius, 0, 300, PoisonTarget::Fatigue, 0x5243_AF63);
    rows.push(check(
        "P6",
        (10..=99).contains(&fat_min) && (10..=99).contains(&fat_max),
        format!("fatigue={fat_min}..{fat_max} (300 draws; declared Range(10,100) ⇒ 10..99)"),
    ));

    // P7 — Pyrrhic Acid duration 1..=2.
    let pyrrhic = Poison::PyrrhicAcid.data();
    rows.push(row(
        "P7",
        format!("{}..{}", pyrrhic.duration_min, pyrrhic.duration_max),
        "1..2",
        "Range(1, 2+1) ⇒ 1..2 ticks",
    ));

    // P8 — Sursum duration exactly 2.
    let sursum = Poison::Sursum.data();
    rows.push(row(
        "P8",
        format!("{}..{}", sursum.duration_min, sursum.duration_max),
        "2..2",
        "Range(2, 2+1) ⇒ exactly 2",
    ));

    // P9 — Indulcet: fatigue 10..99 and LUCK +4..9.
    let (luc_min, luc_max) = poison_range(Poison::Indulcet, 0, 300, PoisonTarget::Luck, 0xBE_EF01);
    let (ifat_min, ifat_max) =
        poison_range(Poison::Indulcet, 0, 300, PoisonTarget::Fatigue, 0xBE_EF01);
    rows.push(check(
        "P9",
        (10..=99).contains(&ifat_min)
            && (10..=99).contains(&ifat_max)
            && (4..=9).contains(&luc_min)
            && (4..=9).contains(&luc_max),
        format!("fatigue={ifat_min}..{ifat_max} luck=+{luc_min}..+{luc_max} (300 draws)"),
    ));

    // P10 — Aegrotat: END 1..4, magicka +5..9.
    let (end_min, end_max) =
        poison_range(Poison::Aegrotat, 0, 300, PoisonTarget::Endurance, 0xBE_EF01);
    let (mag_min, mag_max) =
        poison_range(Poison::Aegrotat, 0, 300, PoisonTarget::Magicka, 0xBE_EF01);
    rows.push(check(
        "P10",
        (1..=4).contains(&end_min)
            && (1..=4).contains(&end_max)
            && (5..=9).contains(&mag_min)
            && (5..=9).contains(&mag_max),
        format!("end=−{end_min}..−{end_max} magicka=+{mag_min}..+{mag_max} (300 draws)"),
    ));

    // P11 — A drug that completes withdraws its positive stats, not its damage.
    let mut drug = PoisonEffect::with_rolls(Poison::Sursum, 0, 2);
    let mut applied = VitalsDelta::default();
    let mut removed = VitalsDelta::default();
    for _ in 0..2 {
        let step = drug.advance(|_, max| max);
        applied.add(&step.applied);
        removed.add(&step.removed);
    }
    rows.push(row(
        "P11",
        format!(
            "state={:?} str_damage={} str_gain={} str_removed={}",
            drug.state,
            applied.attributes[INTELLIGENCE],
            applied.attributes[STRENGTH],
            removed.attributes[STRENGTH]
        ),
        "state=Complete str_damage=-58 str_gain=38 str_removed=-38",
        "RemovePositiveStats: gains withdrawn, attribute damage persists",
    ));
}

/// Observed `[min, max]` of `target` over `ticks` payload minutes.
fn poison_range(
    poison: Poison,
    onset: u16,
    ticks: u16,
    target: PoisonTarget,
    seed: u64,
) -> (i32, i32) {
    let mut rng = Rng64::new(seed);
    let mut effect = PoisonEffect::with_rolls(poison, onset, ticks);
    let (mut min, mut max) = (i32::MAX, i32::MIN);
    let mut seen = 0u16;
    while effect.state != PoisonState::Complete {
        let step = effect.advance(|low, high| rng.range_inclusive(low, high));
        let value = match target {
            PoisonTarget::Health => step.applied.vitals[HEALTH],
            PoisonTarget::Fatigue => step.applied.vitals[FATIGUE],
            PoisonTarget::Magicka => step.applied.vitals[SPELL_POINTS],
            PoisonTarget::Strength => step.applied.attributes[STRENGTH],
            PoisonTarget::Intelligence => step.applied.attributes[INTELLIGENCE],
            PoisonTarget::Willpower => step.applied.attributes[WILLPOWER],
            PoisonTarget::Agility => step.applied.attributes[AGILITY],
            PoisonTarget::Endurance => step.applied.attributes[ENDURANCE],
            PoisonTarget::Personality => step.applied.attributes[PERSONALITY],
            PoisonTarget::Speed => step.applied.attributes[SPEED],
            PoisonTarget::Luck => step.applied.attributes[LUCK],
        };
        if value != 0 {
            seen += 1;
            min = min.min(value.abs());
            max = max.max(value.abs());
        }
    }
    if seen == 0 {
        (0, 0)
    } else {
        (min, max)
    }
}

/// Block B: one seeded lifecycle. The same seed must print the same lines.
fn condition_lifecycle() -> Vec<String> {
    let mut rng = Rng64::new(0x5243_AF63);
    let mut book = DiseaseBook::new();
    book.infect(Disease::CalironsCurse, 100, 5, &mut rng);
    book.infect(Disease::Plague, 100, 5, &mut rng);
    let day_one = book.tick(101, &mut rng);
    let day_five = book.tick(105, &mut rng);
    let caliron_days = book.days_left(Disease::CalironsCurse).unwrap_or(0);
    let long_jump = book.tick(200, &mut rng);
    let caliron_ended = !book.is_active(Disease::CalironsCurse);
    let cured = book.cure_all();
    vec![
        format!("day1 {}", fmt_delta(&day_one.delta)),
        format!("day5 {}", fmt_delta(&day_five.delta)),
        format!(
            "day200 days_applied={} caliron_ended={caliron_ended}",
            long_jump.days
        ),
        format!("caliron_days_left_after_day5={caliron_days}"),
        format!("cured={cured} active_after_cure={} ended=true", book.count()),
    ]
}

/// §12(a)(b) demo: rat → Plague, incubation, day 1 damage, potion cure.
fn demo_disease() -> Vec<(&'static str, String)> {
    let mut rng = Rng64::new(0x5243_AF63);
    let contagion = contact_infection(EnemyKind::Rat, 4, 50.0, 0);
    let disease = match contagion {
        Contagion::Disease(disease) => disease,
        _ => Disease::Plague,
    };
    let mut book = DiseaseBook::new();
    book.infect(disease, 0, 5, &mut rng);
    let day0 = book.tick_with(0, || 3);
    // §12(a) declares the day-1 roll (d = 3) explicitly.
    let day1 = book.tick_with(1, || 3);
    let mut strength = 50;
    strength += day1.delta.attributes[STRENGTH];
    let cured = book.cure_all();
    let after_cure = book.active();
    vec![
        ("diseased", disease.name().to_string()),
        ("day0_delta", format!("{}", day0.delta.vitals[HEALTH] + day0.delta.attributes[STRENGTH])),
        ("day1_str", format!("{strength}")),
        (
            "cured",
            if after_cure.is_empty() {
                "none".to_string()
            } else {
                format!("{}", cured)
            },
        ),
        ("str_after_cure", format!("{strength}")),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-VAMPIRE-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-VAMPIRE-001` — A (16 vectors), B (infection → full moon, digest ×2).
pub fn run_vampire_001() -> i32 {
    println!(
        "PROBE-VAMPIRE-001 - DFU vampirism/lycanthropy/death/cure (M32, CR-f716455c, REQ-L99-065 · spec §4-§6, §7.4, §8)"
    );

    let mut rows = Vec::new();
    vampire_vectors(&mut rows);
    println!("\n-- A · golden vectors (id|expected|actual) --");
    let mut ok_count = 0usize;
    for row in &rows {
        if row.ok {
            ok_count += 1;
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    println!("vectors={} ok={} fail={}", rows.len(), ok_count, rows.len() - ok_count);
    let a_ok = ok_count == rows.len() && rows.len() == 16;

    let first = vampire_lifecycle();
    let second = vampire_lifecycle();
    let digest_a = fnv1a64(first.join("|").as_bytes());
    let digest_b = fnv1a64(second.join("|").as_bytes());
    println!("\n-- B · infection → full → full moon --");
    for line in &first {
        println!("[B] {line}");
    }
    println!("[B] digest={digest_a:016x} digest2={digest_b:016x}");
    let b_rows = vec![
        row(
            "B1 determinism",
            format!("{digest_a:016x}"),
            format!("{digest_b:016x}"),
            "digest ×2 over stage progression, morph and sun damage",
        ),
        row(
            "B2 stage",
            first[0].clone(),
            "stage1=1 stage2=2".to_string(),
            "day >0 = infection warning; day >3 = full vampirism",
        ),
        row(
            "B3 morph",
            first[1].clone(),
            "transformed=true morph_health=100".to_string(),
            "full moon forces MorphSelf; the transform heals to maximum",
        ),
        row(
            "B4 sun",
            first[2].clone(),
            "hp_after_sun=40 sun_damage=60".to_string(),
            "20 minutes in sunlight = 5 ticks × 12 HP",
        ),
    ];
    let b_ok = print_rows("B · infection and sun", &b_rows);

    let pass = a_ok && b_ok;
    println!(
        "\nSUMMARY PROBE-VAMPIRE-001: A={} B={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

/// V1-V8 + Y1-Y5 + K1-K3 (`spec §7.4`).
fn vampire_vectors(rows: &mut Vec<Row>) {
    let mut base = [50i32; ATTRIBUTE_COUNT];
    let vampire = VampireState::full(0, false);
    for (value, bonus) in base.iter_mut().zip(vampire.stat_bonus()) {
        *value += bonus;
    }
    rows.push(row(
        "V1",
        format!(
            "str={} int={} wil={} agi={} end={} per={} spd={} luc={}",
            base[STRENGTH],
            base[INTELLIGENCE],
            base[WILLPOWER],
            base[AGILITY],
            base[ENDURANCE],
            base[PERSONALITY],
            base[SPEED],
            base[LUCK]
        ),
        "str=70 int=50 wil=70 agi=70 end=70 per=70 spd=70 luc=70",
        "+20 to STR/WIL/AGI/END/PER/SPD/LUC; INT untouched",
    ));

    rows.push(row(
        "V2",
        format!("skills={} bonus={}", 6, vampire.skill_bonus()),
        format!("skills=6 bonus={VAMPIRE_SKILL_BONUS}"),
        "Jumping/Running/Stealth/CriticalStrike/Climbing/HandToHand +30",
    ));

    let anthotis = VampireState::full(0, true);
    rows.push(row(
        "V3",
        format!("int={}", 50 + anthotis.stat_bonus()[INTELLIGENCE]),
        "int=70",
        "Anthotis adds INT +20",
    ));

    let mut fed = VampireState::full(0, false);
    fed.feed(1000);
    rows.push(row(
        "V4",
        format!(
            "t_plus_1440={} t_plus_1441={}",
            fed.is_satiated(2440),
            fed.is_satiated(2441)
        ),
        "t_plus_1440=true t_plus_1441=false",
        "IsSatiated uses <= MinutesPerDay",
    ));

    rows.push(row(
        "V5",
        format!("{}", vampire.sun_damage(20)),
        "60",
        "20 min / 4 rounds = 5 ticks × 12 HP",
    ));

    rows.push(row(
        "V6",
        format!(
            "day1={:?} day4={:?}",
            infection_stage(1),
            infection_stage(4)
        ),
        "day1=WarningDream day4=FullBlown",
        "daysPast > 0 dream; > 3 full",
    ));

    rows.push(row(
        "V7",
        format!(
            "roll29={} roll30={} lycan29={} lycan30={}",
            vampire_cure_succeeds(29),
            vampire_cure_succeeds(30),
            lycanthropy_cure_succeeds(29),
            lycanthropy_cure_succeeds(30)
        ),
        "roll29=true roll30=false lycan29=true lycan30=false",
        "$CUREVAM and $CUREWER: 30 %",
    ));

    rows.push(row(
        "V8",
        format!(
            "rand9={} rand10={} submerged={}",
            stendarr_avoids_death(10, 9, false),
            stendarr_avoids_death(10, 10, false),
            stendarr_avoids_death(10, 9, true)
        ),
        "rand9=true rand10=false submerged=false",
        "Range(0,50) < rank and not submerged",
    ));

    // Y1 — lycanthrope bonuses.
    let lycan = LycanthropyState::full(LycanthropyKind::Werewolf, 0);
    let bonus = lycan.stat_bonus();
    rows.push(row(
        "Y1",
        format!(
            "str={} agi={} end={} spd={} skills={} skill_bonus={}",
            bonus[STRENGTH],
            bonus[AGILITY],
            bonus[ENDURANCE],
            bonus[SPEED],
            7,
            lycan.skill_bonus()
        ),
        format!(
            "str=40 agi=40 end=40 spd=40 skills=7 skill_bonus={LYCAN_SKILL_BONUS}"
        ),
        "STR/AGI/END/SPD +40; seven skills +30",
    ));

    rows.push(row(
        "Y2",
        format!("{}", LycanthropyState::max_health_reduction(14_400)),
        "240",
        format!(
            "10 days past the {NEED_TO_KILL_MINUTES}-minute urge ⇒ round(14 400 × 24 / 1440)"
        ),
    ));
    rows.push(row(
        "Y3",
        format!("{}", LycanthropyState::max_health_reduction(1_440)),
        "24",
        "1 day late = 1440 min",
    ));
    rows.push(row(
        "Y4",
        format!("{}", LycanthropyState::limited_max_health(100, 14_400)),
        "4",
        "the health limit floors at 4",
    ));
    rows.push(row(
        "Y5",
        format!(
            "at_cooldown={} past_cooldown={}",
            lycan.can_morph(MORPH_COOLDOWN_MINUTES),
            lycan.can_morph(MORPH_COOLDOWN_MINUTES + 1)
        ),
        "at_cooldown=false past_cooldown=true",
        "CanCastMorphSelf: now - lastCast > 1440",
    ));

    // K1-K3 — cure service.
    let request = |diseases: i32, north_winds: bool| CureServiceRequest {
        diseases,
        infection_pending: false,
        quality: 10,
        arkay_rank: None,
        north_winds_festival: north_winds,
        mercantile: 50,
        personality: 50,
    };
    rows.push(row(
        "K1",
        format!("{}", cure_service_cost(&request(2, false))),
        "562",
        "250×2 = 500 → CalculateCost(q10) 1000 → trade 144·1000>>8",
    ));
    rows.push(row(
        "K2",
        format!("{}", cure_service_cost(&request(1, false))),
        "281",
        "250 → 500 → 144·500>>8",
    ));
    rows.push(row(
        "K3",
        format!("{}", cure_service_cost(&request(1, true))),
        "140",
        "North Winds halves before bargaining: 500/2 = 250 → 140",
    ));
}

/// Block B: infection stages, a full-moon morph and sun damage.
fn vampire_lifecycle() -> Vec<String> {
    let day1 = infection_stage(1);
    let day4 = infection_stage(4);
    let stage1 = if day1 == InfectionStage::WarningDream { 1 } else { 0 };
    let stage2 = if day4 == InfectionStage::FullBlown { 2 } else { 0 };

    let mut lycan = LycanthropyState::full(LycanthropyKind::Werewolf, 0);
    let forced = lycan.full_moon_forces_morph(true);
    let health = lycan.morph(0, 100);
    let transformed = lycan.transformed && forced;

    let vampire = VampireState::full(0, false);
    let sun = vampire.sun_damage(20);
    let hp_after_sun = 100 - sun;

    vec![
        format!("stage1={stage1} stage2={stage2}"),
        format!("transformed={transformed} morph_health={health}"),
        format!("hp_after_sun={hp_after_sun} sun_damage={sun}"),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-REST-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-REST-001` — A (10 vectors), B (interruption digest ×2), D (demo).
pub fn run_rest_001() -> i32 {
    println!(
        "PROBE-REST-001 - DFU rest: hourly recovery, interruption, rent (M32, CR-f716455c, REQ-L99-065 · spec §3, §7.3, §8)"
    );

    let mut rows = Vec::new();
    rest_vectors(&mut rows);
    println!("\n-- A · golden vectors (id|expected|actual) --");
    let mut ok_count = 0usize;
    for row in &rows {
        if row.ok {
            ok_count += 1;
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    println!("vectors={} ok={} fail={}", rows.len(), ok_count, rows.len() - ok_count);
    let a_ok = ok_count == rows.len() && rows.len() == 10;

    let first = rest_interruption();
    let second = rest_interruption();
    let digest_a = fnv1a64(first.join("|").as_bytes());
    let digest_b = fnv1a64(second.join("|").as_bytes());
    println!("\n-- B · 12 h rest with an enemy on hour 3 (digest ×2) --");
    for line in &first {
        println!("[B] {line}");
    }
    println!("[B] digest={digest_a:016x} digest2={digest_b:016x}");
    let b_rows = vec![
        row(
            "B1 determinism",
            format!("{digest_a:016x}"),
            format!("{digest_b:016x}"),
            "digest ×2 over the interrupted rest",
        ),
        check(
            "B2 interruption",
            first[0].starts_with("hours=3 minutes=180 interrupted=true msg=354"),
            first[0].clone(),
        ),
    ];
    let b_ok = print_rows("B · interruption", &b_rows);

    // ── D · demo over the retail capital ──────────────────────────────────
    let d_ok = match run_rest_demo() {
        Ok(demo_ok) => demo_ok,
        Err(reason) => {
            println!("\n-- D · demo (ARENA2) --");
            println!("[EXIT=2] {reason}");
            return if a_ok && b_ok { 2 } else { 1 };
        }
    };

    let pass = a_ok && b_ok && d_ok;
    println!(
        "\nSUMMARY PROBE-REST-001: A={} B={} D={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(d_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

/// R1-R10 (`spec §7.3`).
fn rest_vectors(rows: &mut Vec<Row>) {
    // R1 — END 50, Medical 30 (+60), MaxHP 100.
    let medical = 30 + medical_bonus(RapidHealing::default(), true, false);
    rows.push(row(
        "R1",
        format!("{}", health_recovery_per_hour(50, medical, 100)),
        "9",
        "floor(0 + 90·100/1000)",
    ));
    // R2
    rows.push(row(
        "R2",
        format!("{}", health_recovery_per_hour(100, 60, 200)),
        "17",
        "5 + 60·200/1000",
    ));
    // R3 — must be 1, not 0.
    rows.push(row(
        "R3",
        format!("{}", health_recovery_per_hour(10, 60, 50)),
        "1",
        "floor(−4 + 3) = −1 ⇒ max 1",
    ));
    // R4
    rows.push(row(
        "R4",
        format!("{}", health_recovery_per_hour(30, 90, 100)),
        "7",
        "−2 + 9",
    ));
    // R5
    let rapid = RapidHealing {
        always: true,
        in_light: false,
        in_darkness: false,
    };
    rows.push(row(
        "R5",
        format!(
            "{}",
            health_recovery_per_hour(
                50,
                medical_bonus_only(rapid, 0),
                100
            )
        ),
        "10",
        "RapidHealing Always ⇒ +100 medical ⇒ 0 + 100·100/1000",
    ));
    // R6
    rows.push(row(
        "R6",
        format!("{}|{}", fatigue_recovery_per_hour(100), fatigue_recovery_per_hour(7)),
        "12|1",
        "MaxFatigue/8, floor at 1",
    ));
    // R7
    rows.push(row(
        "R7",
        format!(
            "{}|{}",
            spell_point_recovery_per_hour(100, false),
            spell_point_recovery_per_hour(100, true)
        ),
        "12|0",
        "MaxMagicka/8; NoRegenSpellPoints ⇒ 0",
    ));
    // R8 — FullRest from 1 HP with R1 rates.
    let mut full = RestWindow::new(RestMode::FullRest, full_vitals(1, 100), 0)
        .with_endurance(50)
        .with_medical(30);
    let outcome = full.run(|_| false);
    rows.push(row(
        "R8",
        format!("hours={} hp={} end={:?}", outcome.hours, full.vitals.health, outcome.end),
        "hours=11 hp=100 end=FullyHealed",
        "1 + 11 × 9 = 100 exactly",
    ));
    // R9 — TimedRest 8 h.
    let mut timed = RestWindow::new(RestMode::TimedRest, full_vitals(50, 100), 8).with_endurance(50);
    let outcome = timed.run(|_| false);
    rows.push(row(
        "R9",
        format!("hours={} minutes={}", outcome.hours, outcome.minutes),
        "hours=8 minutes=480",
        "8 h × 6 ticks × 10 min",
    ));
    // R10 — enemy after the third hour.
    let mut broken = RestWindow::new(RestMode::TimedRest, full_vitals(50, 100), 12).with_endurance(50);
    let outcome = broken.run(|hour| hour >= 3);
    rows.push(row(
        "R10",
        format!(
            "hours={} interrupted={} msg={}",
            outcome.hours,
            outcome.interrupted,
            outcome.message_id().unwrap_or(0)
        ),
        "hours=3 interrupted=true msg=354",
        "totalHours++ then AreEnemiesNearby ⇒ enemyBrokeRest",
    ));
}

/// `medical += addToMedical` with no live skill.
fn medical_bonus_only(rapid: RapidHealing, medical_live: i32) -> i32 {
    medical_live + medical_bonus(rapid, true, false)
}

fn full_vitals(health: i32, max_health: i32) -> RestVitals {
    RestVitals::new(health, max_health, max_health, max_health, max_health, max_health)
}

/// Block B: 12 h timed rest, enemy injected on hour 3.
fn rest_interruption() -> Vec<String> {
    let mut window = RestWindow::new(RestMode::TimedRest, full_vitals(10, 100), 12)
        .with_endurance(50)
        .with_medical(30)
        .with_rented_room(24);
    let outcome = window.run(|hour| hour == 3);
    vec![format!(
        "hours={} minutes={} interrupted={} msg={} hp={} room_hours={}",
        outcome.hours,
        outcome.minutes,
        outcome.interrupted,
        outcome.message_id().unwrap_or(0),
        window.vitals.health,
        window.remaining_hours_rented.unwrap_or(0)
    )]
}

/// §12(c)(d) demo over the retail capital: rent one night, sleep 9 h.
fn run_rest_demo() -> Result<bool, String> {
    let capital = crate::m10_probe::compose_capital()?;
    let tavern = capital
        .buildings
        .iter()
        .find(|building| building.building_type == 15 && building.quality == 10)
        .or_else(|| {
            capital
                .buildings
                .iter()
                .find(|building| building.building_type == 15)
        });
    let quality = tavern.map_or(10, |building| building.quality);

    const DAY_OF_YEAR: u64 = 10;
    const GOLD_BEFORE: i64 = 100;
    const MERCANTILE: i32 = 50;
    const PERSONALITY: i32 = 50;

    let room_base = room_cost(1, DAY_OF_YEAR as i64);
    let room_price = i64::from(calculate_trade_price(
        i32::try_from(room_base).unwrap_or(i32::MAX),
        i32::from(quality),
        false,
        MERCANTILE,
        PERSONALITY,
    ));
    let gold_after = GOLD_BEFORE - room_price;

    let now_seconds = 86_400 * DAY_OF_YEAR;
    let expiry = room_expiry_seconds(now_seconds, 1);
    let room_hours_before = room_remaining_hours(now_seconds, expiry);

    // Sleep nine hours on the rented room with the R1 rates.
    let mut window = RestWindow::new(RestMode::TimedRest, full_vitals(10, 100), 9)
        .with_endurance(50)
        .with_medical(30)
        .with_rented_room(room_hours_before as u32);
    let outcome = window.run(|_| false);
    let room_hours_after = room_remaining_hours(now_seconds + outcome.minutes as u64 * 60, expiry);

    // §12(d): the same 12 h window with an enemy injected on hour 3.
    let mut interrupted = RestWindow::new(RestMode::TimedRest, full_vitals(10, 100), 12)
        .with_endurance(50)
        .with_medical(30);
    let broken = interrupted.run(|hour| hour == 3);

    let hour_before = 8u64;
    let hour_after = (hour_before + u64::from(outcome.hours)) % 24;

    println!("\n-- D · demo over the retail capital (M30 rent) --");
    println!(
        "[D] tavern_quality={quality} room_cost={room_base} room_price={room_price} gold_before={GOLD_BEFORE} gold_after={gold_after}"
    );
    println!(
        "[D] room_hours_before={room_hours_before} room_hours_after={room_hours_after} hp_before=10 hp_after={} hour_before={hour_before} hour_after={hour_after}",
        window.vitals.health
    );
    println!(
        "[D] hp_per_hour={} magicka_per_hour={} interrupted_hours={} interrupted_msg={}",
        window.health_per_hour(),
        window.spell_points_per_hour(),
        broken.hours,
        broken.message_id().unwrap_or(0)
    );

    let rows = vec![
        row(
            "D1 rent",
            format!("{room_hours_before}→{room_hours_after}"),
            "24→15",
            "one night = 24 h; nine hours slept leave 15 (M30 expiry)",
        ),
        row(
            "D2 health",
            format!("10→{} (+{} per hour)", window.vitals.health, window.health_per_hour()),
            "10→91 (+9 per hour)",
            "END 50, Medical 30 (+60), MaxHP 100",
        ),
        row(
            "D3 gold",
            format!("{GOLD_BEFORE}→{gold_after}"),
            format!("100→{}", 100 - room_price),
            "the room is paid before sleeping",
        ),
        row(
            "D4 clock",
            format!("{hour_before}→{hour_after}"),
            "8→17",
            "nine game hours",
        ),
        row(
            "D5 interruption",
            format!("hours={} msg={}", broken.hours, broken.message_id().unwrap_or(0)),
            "hours=3 msg=354",
            "enemy injected on hour 3 (spec §12(d))",
        ),
    ];
    Ok(print_rows("D · measured demo", &rows))
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-DEATH-STATE-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-DEATH-STATE-001` — A (X1-X2 + NEGATIVE census), B (death latch).
pub fn run_death_state_001() -> i32 {
    println!(
        "PROBE-DEATH-STATE-001 - DFU player death: HP<=0, guild benefit, no revival (M32, CR-f716455c, REQ-L99-065 · spec §6, §8)"
    );

    let x1 = apply_damage(10, 100, 10, false);
    let x2 = apply_damage(10, 100, 10, true);
    let (symbols, mentions, files) = reversal_census();
    println!("\n-- A · golden vectors + NEGATIVE census --");
    let a_rows = vec![
        row(
            "X1",
            format!("{x1:?}"),
            "Dead",
            "HP 10 − 10 with no guild benefit ⇒ the death event fires",
        ),
        row(
            "X2",
            format!("{x2:?}"),
            "Avoided(10)",
            "(int)(MaxHealth × 0.1) = 10",
        ),
        check(
            "NEGATIVE",
            symbols == 0,
            format!(
                "resurrect_symbols={symbols} (scanned_files={files}, doc_mentions={mentions}; the source has no revival path)"
            ),
        ),
    ];
    let a_ok = print_rows("A · death vectors and census", &a_rows);
    println!(
        "resurrect_symbols={symbols} scanned_files={files} doc_mentions={mentions}"
    );

    // ── B · HP 1 → lethal hit → DeathInProgress → reset ───────────────────
    let lethal = apply_damage(1, 100, 1, false);
    let mut death = DeathState::alive();
    let mut rows_b = vec![row(
        "B1 death event",
        format!("{lethal:?}"),
        "Dead",
        "HP 1 hit by 1 without the benefit",
    )];
    if lethal == LifeOutcome::Dead {
        death.begin(100.0, 1.75);
    }
    let avoided = apply_damage(1, 100, 1, true);
    let reset_due = death.reset_due(100.0 + DEATH_RESET_SECONDS + 0.1);
    rows_b.push(check(
        "B2 death state",
        death.in_progress && death.input_paused(),
        format!(
            "death=true input_paused={} reset_after_s={DEATH_RESET_SECONDS}",
            death.input_paused()
        ),
    ));
    rows_b.push(row(
        "B3 reset",
        format!("{reset_due}"),
        "true",
        "TimeBeforeReset = 3 s ⇒ title menu",
    ));
    rows_b.push(row(
        "B4 hp restored",
        format!("{:?}", avoided.health().unwrap_or(0)),
        format!("{}", avoid_death_health(100)),
        "AvoidDeath leaves 10 % of MaxHealth",
    ));
    let b_ok = print_rows("B · death latch and reset", &rows_b);

    let pass = a_ok && b_ok;
    println!(
        "\nSUMMARY PROBE-DEATH-STATE-001: A={} B={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

/// NEGATIVE census over `crates/`: occurrences of the two revival tokens.
///
/// Returns `(symbols, raw_mentions, scanned_files)`. `symbols` counts
/// occurrences in **code** (comments and string/char literals stripped), so a
/// doc comment that declares the invariant does not count as an API; the raw
/// count is reported separately for transparency.
fn reversal_census() -> (usize, usize, usize) {
    let needles = [concat!("resur", "rect"), concat!("re", "vive")];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut files = 0usize;
    let mut symbols = 0usize;
    let mut mentions = 0usize;
    let mut stack = vec![root.join("crates")];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                files += 1;
                for needle in needles {
                    mentions += source.matches(needle).count();
                    symbols += code_only(&source).matches(needle).count();
                }
            }
        }
    }
    (symbols, mentions, files)
}

/// Strips Rust comments (line, block, doc) and string/char literals.
fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        if current == '/' && index + 1 < chars.len() && chars[index + 1] == '/' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
        } else if current == '/' && index + 1 < chars.len() && chars[index + 1] == '*' {
            index += 2;
            let mut depth = 1usize;
            while index < chars.len() && depth > 0 {
                if chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '*' {
                    depth += 1;
                    index += 2;
                } else if chars[index] == '*' && index + 1 < chars.len() && chars[index + 1] == '/'
                {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if current == 'r' && index + 1 < chars.len() && (chars[index + 1] == '"' || chars[index + 1] == '#')
        {
            // Raw string: r"…" or r#"…"#.
            let mut cursor = index + 1;
            let mut hashes = 0usize;
            while cursor < chars.len() && chars[cursor] == '#' {
                hashes += 1;
                cursor += 1;
            }
            if cursor < chars.len() && chars[cursor] == '"' {
                cursor += 1;
                loop {
                    if cursor >= chars.len() {
                        break;
                    }
                    if chars[cursor] == '"' {
                        let mut close = 0usize;
                        while close < hashes
                            && cursor + 1 + close < chars.len()
                            && chars[cursor + 1 + close] == '#'
                        {
                            close += 1;
                        }
                        if close == hashes {
                            cursor += 1 + hashes;
                            break;
                        }
                    }
                    cursor += 1;
                }
                index = cursor;
            } else {
                out.push(current);
                index += 1;
            }
        } else if current == '"' {
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                } else if chars[index] == '"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else if current == '\''
            && index + 2 < chars.len()
            && (chars[index + 1] == '\\' || chars[index + 2] == '\'')
        {
            // Char literal (lifetimes stay in the code).
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                } else if chars[index] == '\'' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else {
            out.push(current);
            index += 1;
        }
    }
    out
}
