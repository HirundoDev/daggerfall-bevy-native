//! `m42_law_probe` — `PROBE-SYS-LAW-001` (M42 / W42-4): crime/law in depth —
//! per-region record, legal reputation, guards, and the five outcomes
//! measured one by one (roadmap `docs/acknowledgments/roadmap-m18-m48.md:320`).
//!
//! This probe **extends** M28's `PROBE-CRIME-001` + `PROBE-LAW-001`
//! (`crates/app/src/m28_probe.rs`) instead of repeating them: M28 proves the
//! court tables, the sentence formulas, the prison clock and the release
//! (GV-28 golden vectors), so the rows here are the **W42-4 depth layer** the
//! M28 walk lacks: (A) per-region record — 62 regions, clamps, People faction
//! propagation per region; (B) legal reputation — loss table per crime,
//! half-propagate, sentence raise rule; (C) guards — spawn contract quoted per
//! scene (threshold/quota), the countdown path, the hit → dialogue rule;
//! (D) the five outcomes measured one by one: fine, prison, escape, bribe,
//! death — each one paired with its source anchor or its declared rule;
//! (E) adversarial NEGATIVE rows with on-disk evidence paths.
//!
//! Citations: `crates/behavior/src/law.rs` ports
//! `Game/Entities/PlayerEntity.cs` (`:2263-2281`, `:2284`, `:2291-2299`,
//! `:2301-2311`, `:2313-2332`, `:621-745`, `:2247-2256`) and
//! `Game/UserInterfaceWindows/DaggerfallCourtWindow.cs` (`:57-60`, `:148-165`,
//! `:262-290`) behind `$DF_APP_ROOT` paths where they resolve on disk.
//! SR-42-4-1 (bribe price = court row base) is a **declared** W42-4 rule: the
//! DFU court has no bribe path, so it is marked `rule=SR-42-4-1` instead of a
//! `src=` line — the falsifier `bribe-price` pins that declaration.
//!
//! **0 retail bytes**: every fixture is synthetic, so the probe is
//! retail-independent and never prints `NO DATA` for a missing `ARENA2`.
//!
//! **Determinism:** no floats are printed, all iteration is over fixed arrays
//! and seeded `DfRandom` draws, and the last line is `DIGEST <sha256>` over
//! every byte printed before it.

use daggerfall_antibodies::crime_guardrails::check_prison_clock;
use daggerfall_behavior::law::{
    bribe_guard, bribe_price, Crimes, CourtSession, CourtState, DfRandom,
    GuildRescue, GuardSpawnContext, LawOutcome, PlayerLaw, PrisonState, CRIME_TRIGGERS,
    FINE_STEP, GUARD_COUNTDOWN_MAX_SECONDS, GUARD_COUNTDOWN_MIN_SECONDS,
    INTERIOR_GUARD_MAX, INTERIOR_GUARD_MIN, MAX_ACTIVE_GUARD_SPAWNS, MAX_LEGAL_REP,
    MIN_LEGAL_REP, PEOPLE_FACTION_TYPE, PUNISHMENT_FINE_OR_PRISON, PUNISHMENT_SEVERE,
    REGION_COUNT_HINT, SEVERE_PUNISHMENT_EXECUTED,
};
use daggerfall_behavior::law::{
    guard_hit_response, guard_spawn_request, punishment_type, sentence, surrender_to_city_guards,
};
use daggerfall_behavior::factions::FactionGraph;
use sha2::{Digest, Sha256};

/// Lane correlation id (W42-4, same lane as W42-1..W42-3).
const CR: &str = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3";

/// Citations (every one resolves on disk).
const SRC_LAW: &str = "crates/behavior/src/law.rs";
const SRC_GUARDRAILS: &str = "crates/antibodies/src/crime_guardrails.rs";
const SRC_PE: &str = "reference/dfu-spec/Assets/Scripts/Game/Entities/PlayerEntity.cs";
const SRC_COURT: &str =
    "reference/dfu-spec/Assets/Scripts/Game/UserInterfaceWindows/DaggerfallCourtWindow.cs";
const SRC_ENEMY: &str = "reference/dfu-spec/Assets/Scripts/Game/EnemyAttack.cs";
const SPEC_M28: &str =
    "docs/sintergic/game/programming/m28-crime-law-courts-spec-CR-5243af63.md";
const SPEC_ROADMAP: &str = "docs/acknowledgments/roadmap-m18-m48.md:320";
const RULE_BRIBE: &str = "rule=SR-42-4-1";

/// One printed evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

/// Row whose verdict is computed by the caller.
fn check(id: impl Into<String>, ok: bool, detail: String) -> Row {
    Row {
        id: id.into(),
        ok,
        detail,
    }
}

/// Hexadecimal `sha256` (the `DIGEST` line and nothing else needs it).
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Region 0 People faction (`type: 15`) — the propagation target the law
/// halves into (`PlayerEntity.cs:2291-2299`).
const PEOPLE_GRAPH: &str = "#77\ntype: 15\nregion: 1\nname: People\nally: 78\nenemy: 79\n#78\ntype: 2\nname: Ally\n#79\ntype: 2\nname: Enemy\n";

fn people_graph() -> FactionGraph {
    FactionGraph::parse(PEOPLE_GRAPH, 0x1234).expect("valid fixture")
}

fn region_graph(regions: usize) -> (PlayerLaw, FactionGraph) {
    (PlayerLaw::new(regions), people_graph())
}

// ── A · per-region record ─────────────────────────────────────────────

fn block_a(out: &mut String) -> bool {
    let (mut law, _) = region_graph(62);
    out.push_str(&format!(
        "REGIONS count={} crime={} arrested={} in_prison={}\n",
        law.regions().len(),
        law.crime.value(),
        law.arrested,
        law.in_prison,
    ));
    let (mut law2, _) = region_graph(1);
    let clamped_lo = law2.region_mut(0).expect("region 0").set_legal_rep(-101);
    let clamped_hi = law2.region_mut(0).expect("region 0").set_legal_rep(101);
    out.push_str(&format!(
        "REGION law0_legal={} clamp_lo={clamped_lo} clamp_hi={clamped_hi} flags0={}\n",
        law.region(0).expect("region 0").legal_rep,
        law.region(0).expect("region 0").severe_punishment_flags,
    ));
    let rows = vec![
        check(
            "region-count",
            law.regions().len() == REGION_COUNT_HINT,
            format!(
                "retail has 62 regions src={SRC_LAW} spec={SPEC_M28} (ports {SRC_PE}:2247-2256; spec §1.1)"
            ),
        ),
        check(
            "region-neutral",
            law.region(0).expect("region 0").legal_rep == 0
                && law.region(0).expect("region 0").severe_punishment_flags == 0,
            format!("neutral record, no flags src={SRC_LAW}"),
        ),
        check(
            "region-clamp",
            clamped_lo == MIN_LEGAL_REP && clamped_hi == MAX_LEGAL_REP,
            format!("-101=>-100 +101=>+100 (`ClampLegalReputations`) src={SRC_LAW} (ports {SRC_PE}:2247-2256)"),
        ),
        check(
            "record-fields",
            law.crime == Crimes::None && !law.arrested && !law.in_prison,
            format!("crime/arrested/in_prison carried on PlayerLaw src={SRC_LAW} (ports {SRC_PE}:101,1580)"),
        ),
    ];
    emit(out, &rows)
}

// ── B · legal reputation ──────────────────────────────────────────────

fn block_b(out: &mut String) -> bool {
    let (mut law, mut graph) = region_graph(2);
    let losses: Vec<i16> = Crimes::ALL.iter().map(|c| c.reputation_loss()).collect();
    let loss_line = losses
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    out.push_str(&format!("REPLOSS {loss_line}\n"));
    law.set_crime(Crimes::Murder, false);
    let delta = law
        .lower_rep_for_crime(0, Crimes::Murder, &mut graph)
        .expect("region 0");
    out.push_str(&format!(
        "LOWER crime=5 legal_after={} people_faction={:?} people_delta={}\n",
        delta.legal_rep_after, delta.people_faction, delta.people_delta,
    ));
    law.half_of_legal_rep_lost_from_crime = Crimes::Murder.reputation_loss() / 2;
    let raised = law
        .raise_rep_for_sentence(0, &mut graph)
        .expect("region 0");
    out.push_str(&format!(
        "RAISE half=10 gain=9 legal_after={} people_delta={}\n",
        raised.legal_rep_after, raised.people_delta,
    ));
    let rows = vec![
        check(
            "rep-loss-table",
            loss_line == "0,10,5,10,8,20,10,2,1,2,2,75,2,8,36,10",
            format!("16 losses (`reputationLossPerCrime`) src={SRC_LAW} (ports {SRC_PE}:2284)"),
        ),
        check(
            "rep-half-propagate",
            delta.legal_rep_after == -20
                && delta.people_delta == -10
                && delta.people_faction == Some(77),
            format!(
                "Murder -20, half -10 to People(77) src={SRC_LAW} spec={SPEC_M28} (ports {SRC_PE}:2291-2299; spec §1.1)"
            ),
        ),
        check(
            "rep-sentence-raise",
            raised.legal_rep_after == -11 && raised.people_delta == 4,
            format!("+9 region, +4 People (`half-1`, `(half-1)/2`) src={SRC_LAW} (ports {SRC_PE}:2301-2311)"),
        ),
    ];
    emit(out, &rows)
}

// ── C · guards ────────────────────────────────────────────────────────

fn block_c(out: &mut String) -> bool {
    let mut interior_hits = 0usize;
    let mut interior_guards = vec![];
    for seed in 0..64u32 {
        if let Some(daggerfall_behavior::law::GuardSpawnRequest::Interior { guards, .. }) =
            guard_spawn_request(
                GuardSpawnContext {
                    inside_building: true,
                    ..GuardSpawnContext::default()
                },
                &mut DfRandom::srand(seed),
            )
        {
            interior_hits += 1;
            interior_guards.push(guards);
        }
    }
    let min_g = interior_guards.iter().copied().min().unwrap_or(0);
    let max_g = interior_guards.iter().copied().max().unwrap_or(0);
    // Countdown path: seen by a non-guard NPC.
    let mut countdowns = vec![];
    for seed in 0..64u32 {
        if let Some(daggerfall_behavior::law::GuardSpawnRequest::Countdown { seconds }) =
            guard_spawn_request(
                GuardSpawnContext {
                    seen_by_npc: true,
                    ..GuardSpawnContext::default()
                },
                &mut DfRandom::srand(seed),
            )
        {
            countdowns.push(seconds);
        }
    }
    let min_s = countdowns.iter().copied().min().unwrap_or(0);
    let max_s = countdowns.iter().copied().max().unwrap_or(0);
    // Quota: more than five active guards refuses.
    let refused = guard_spawn_request(
        GuardSpawnContext {
            active_guards: MAX_ACTIVE_GUARD_SPAWNS + 1,
            immediate: true,
            npcs_in_radius: 3,
            ..GuardSpawnContext::default()
        },
        &mut DfRandom::srand(1),
    )
    .is_none();
    let first = guard_hit_response(Crimes::Murder, false, true, false);
    let second = guard_hit_response(Crimes::Murder, true, true, false);
    let dialogues = usize::from(first.show_surrender_dialogue)
        + usize::from(second.show_surrender_dialogue);
    out.push_str(&format!(
        "GUARDS interior_hits={interior_hits}/64 guards={min_g}..{max_g} countdown={min_s}..{max_s} quota_refused={refused} dialogues={dialogues}\n",
    ));
    let rows = vec![
        check(
            "guard-interior",
            interior_hits == 64 && min_g >= INTERIOR_GUARD_MIN && max_g <= INTERIOR_GUARD_MAX,
            format!(
                "indoors always spawns {INTERIOR_GUARD_MIN}..={INTERIOR_GUARD_MAX} src={SRC_LAW} (ports {SRC_PE}:630-641)"
            ),
        ),
        check(
            "guard-countdown",
            countdowns.len() == 64
                && min_s >= GUARD_COUNTDOWN_MIN_SECONDS
                && max_s <= GUARD_COUNTDOWN_MAX_SECONDS,
            format!(
                "NPC witness ⇒ countdown {GUARD_COUNTDOWN_MIN_SECONDS}..={GUARD_COUNTDOWN_MAX_SECONDS}s src={SRC_LAW} (ports {SRC_PE}:737-742)"
            ),
        ),
        check(
            "guard-quota",
            refused,
            format!(
                ">{MAX_ACTIVE_GUARD_SPAWNS} actives ⇒ no spawn src={SRC_GUARDRAILS} src={SRC_LAW} (ports {SRC_PE}:623-626)"
            ),
        ),
        check(
            "guard-dialogue-once",
            dialogues == 1 && first.lower_legal_rep && !second.lower_legal_rep,
            format!("rep drop + dialogue at most once src={SRC_ENEMY}:270-296 src={SRC_LAW}"),
        ),
    ];
    emit(out, &rows)
}

// ── D · the five outcomes, measured one by one ────────────────────────

fn block_d(out: &mut String) -> bool {
    // D1 · fine: a seed whose sentence has punishment type 2 with 0 prison days
    // would depend on the split draws, so the probe drives a guilty session on
    // the row and measures the halving rule directly.
    let gold = 1_000_000i32;
    let seed = (0..4_000u32)
        .find(|seed| {
            punishment_type(-50, &mut DfRandom::srand(*seed)) == PUNISHMENT_FINE_OR_PRISON
        })
        .expect("a type-2 seed");
    let expected = sentence(-50, Crimes::Murder, gold, &mut DfRandom::srand(seed))
        .expect("court row");
    let mut fine_session = CourtSession::open(
        Crimes::Murder,
        0,
        -50,
        gold,
        GuildRescue::default(),
        &mut DfRandom::srand(seed),
    );
    fine_session.answer_guilty();
    out.push_str(&format!(
        "FINE outcome={:?} seed={seed} fine={} paid={} days={} left={} steps={} identity={}\n",
        LawOutcome::Fine,
        expected.fine,
        fine_session.fine_paid,
        expected.days_in_prison,
        fine_session.days_left,
        expected.steps,
        expected.fine / FINE_STEP + expected.days_in_prison / 3 == expected.steps,
    ));
    // D2 · prison: serve the days the guilty plea left, then raise.
    let days = fine_session.days_left;
    let mut prison = PrisonState::begin(days);
    let mut ended = false;
    for _ in 0..days.max(0) {
        ended = prison.tick_day();
    }
    let clock_ok =
        check_prison_clock(days, prison.clock_advanced, false).is_ok();
    out.push_str(&format!(
        "PRISON outcome={:?} days={days} ended={ended} clock={} clock_ok={clock_ok}\n",
        LawOutcome::Prison,
        prison.clock_advanced,
    ));
    // D3 · escape: refused surrender (alive, no court).
    let refused = surrender_to_city_guards(-30, 10, false, &mut DfRandom::srand(1));
    let escape_is_refusal = refused.is_some_and(|o| !o.court && o.health_after == 1);
    out.push_str(&format!(
        "ESCAPE outcome={:?} legal=-30 voluntary=false court={} health={} escape={escape_is_refusal}\n",
        LawOutcome::Escape,
        refused.map(|o| o.court).unwrap_or(true),
        refused.map(|o| o.health_after).unwrap_or(-1),
    ));
    // D4 · bribe: declared rule SR-42-4-1 (NOT retail).
    let price = bribe_price(Crimes::Theft);
    let accepted = bribe_guard(Crimes::Theft, 200);
    let short = bribe_guard(Crimes::Theft, 199);
    let bribe_ok = price == Some(200)
        && accepted.is_some_and(|o| o.accepted && o.gold_paid == 200 && o.crime_after == Crimes::None)
        && short.is_some_and(|o| !o.accepted && o.crime_after == Crimes::Theft);
    out.push_str(&format!(
        "BRIBE outcome={:?} price={:?} paid200={:?} paid199={:?} {RULE_BRIBE}\n",
        LawOutcome::Bribe,
        price,
        accepted.map(|o| (o.accepted, o.gold_paid, o.crime_after.value())),
        short.map(|o| (o.accepted, o.crime_after.value())),
    ));
    // D5 · death: the dead player never surrenders; the Executed state flags
    // the region with bit 2.
    let dead = surrender_to_city_guards(-30, 0, true, &mut DfRandom::srand(1));
    let (mut law, _) = region_graph(1);
    law.set_crime(Crimes::Murder, false);
    let mut death_session = CourtSession {
        state: CourtState::Executed,
        region: 0,
        crime: Crimes::Murder,
        legal_rep: -100,
        sentence: None,
        rescued_by: None,
        fine_paid: 0,
        days_left: 0,
        raise_reputation: false,
        refill_vitals: false,
        reposition: true,
    };
    death_session.apply_severe_punishment(law.region_mut(0).expect("region 0"));
    let flagged = law.region(0).expect("region 0").executed();
    out.push_str(&format!(
        "DEATH outcome={:?} dead_surrender_none={} executed_flag={flagged}\n",
        LawOutcome::Death,
        dead.is_none(),
    ));
    let rows = vec![
        check(
            "outcome-fine",
            fine_session.state == CourtState::ServingSentence
                || fine_session.state == CourtState::Done,
            format!(
                "guilty ⇒ fine {}>>1={} (type 2) src={SRC_LAW} (ports {SRC_COURT}:332-333)",
                expected.fine, fine_session.fine_paid,
            ),
        ),
        check(
            "outcome-fine-halved",
            fine_session.fine_paid == expected.fine >> 1
                && fine_session.days_left == expected.days_in_prison >> 1,
            format!(
                "halving: paid={} left={} src={SRC_LAW} (ports {SRC_COURT}:332-333)",
                fine_session.fine_paid, fine_session.days_left,
            ),
        ),
        check(
            "outcome-prison",
            (days <= 0 && !ended) || (days > 0 && ended && clock_ok),
            format!(
                "days={days} clock={} = days*86400 src={SRC_LAW} (ports {SRC_COURT}:464-479)",
                prison.clock_advanced,
            ),
        ),
        check(
            "outcome-escape",
            escape_is_refusal,
            format!("refused ⇒ alive, no court, guards keep chasing src={SRC_LAW} (ports {SRC_PE}:2313-2332)"),
        ),
        check(
            "outcome-bribe",
            bribe_ok,
            format!("price 200, 200 clears / 199 refuses {RULE_BRIBE} (declared, not retail)"),
        ),
        check(
            "outcome-death",
            dead.is_none() && flagged,
            format!(
                "dead ⇒ None src={SRC_LAW} (ports {SRC_PE}:2313-2332); Executed flags bit {SEVERE_PUNISHMENT_EXECUTED} src={SRC_LAW} (ports {SRC_COURT}:279-290)"
            ),
        ),
    ];
    emit(out, &rows)
}

// ── E · adversarial (non-goals, negative evidence) ────────────────────

fn block_e(out: &mut String) -> bool {
    let none_court = CourtSession::open(
        Crimes::None,
        0,
        -100,
        0,
        GuildRescue::default(),
        &mut DfRandom::srand(5),
    );
    let type2_in_40 = (0..40u32)
        .filter(|seed| {
            punishment_type(-50, &mut DfRandom::srand(*seed)) == PUNISHMENT_FINE_OR_PRISON
        })
        .count();
    let severe_in_40 = (0..40u32)
        .filter(|seed| {
            punishment_type(-50, &mut DfRandom::srand(*seed)) == PUNISHMENT_SEVERE
        })
        .count();
    let seven = CRIME_TRIGGERS.len();
    let (mut law, mut graph) = region_graph(1);
    law.set_crime(Crimes::Murder, false);
    let court = law
        .open_court(0, 1_000_000, GuildRescue::default(), &mut DfRandom::srand(9))
        .expect("region 0");
    out.push_str(&format!(
        "NEGATIVE none-court-no-table reason={SRC_LAW} none_court={} none_closed={} type2_in_40={type2_in_40} severe_in_40={severe_in_40} triggers={seven}\n",
        none_court.sentence.is_none(),
        none_court.state == CourtState::Done,
    ));
    out.push_str(&format!(
        "NEGATIVE open-court-arrest-path reason={SRC_LAW} open_court_arrested={} half={} region={} crime={}\n",
        law.arrested,
        law.half_of_legal_rep_lost_from_crime,
        court.region,
        court.crime.value(),
    ));
    out.push_str(&format!("NEGATIVE retail-bribe-path reason={SPEC_M28} no_retail_bribe=true declared={RULE_BRIBE}\n"));
    let _ = graph;
    let rows = vec![
        check(
            "negative-none-no-table",
            none_court.sentence.is_none() && none_court.state == CourtState::Done,
            format!("None closes before the table src={SRC_LAW} (ports {SRC_COURT}:109-113)"),
        ),
        check(
            "negative-type-varies",
            type2_in_40 > 0 && severe_in_40 > 0,
            format!("type is not constant over 40 seeds src={SRC_LAW} (ports {SRC_COURT}:122-138)"),
        ),
        check(
            "negative-seven-triggers",
            seven == 7,
            format!("spec §1.3 census src={SRC_LAW} spec={SPEC_M28}"),
        ),
    ];
    emit(out, &rows)
}

fn emit(out: &mut String, rows: &[Row]) -> bool {
    let mut ok = true;
    for row in rows {
        if row.ok {
            out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
            ok &= true;
        } else {
            out.push_str(&format!("[FAIL] {} got=fail want=ok - {}\n", row.id, row.detail));
            ok = false;
        }
    }
    ok
}

/// `--probe SYS-LAW-001`: blocks A–E, `EXIT=0` ⟺ every row OK.
pub fn run_sys_law_001() -> i32 {
    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-LAW-001 - per-region record, legal reputation, guards, five outcomes (M42/W42-4, {CR})\nroadmap={SPEC_ROADMAP} spec={SPEC_M28}\n"
    ));

    let mut ok = true;
    out.push_str("\n-- A · per-region record (62 regions, clamps) --\n");
    ok &= block_a(&mut out);
    out.push_str("\n-- B · legal reputation (loss, half-propagate, sentence raise) --\n");
    ok &= block_b(&mut out);
    out.push_str("\n-- C · guards (interior, countdown, quota, dialogue-once) --\n");
    ok &= block_c(&mut out);
    out.push_str("\n-- D · five outcomes measured one by one --\n");
    ok &= block_d(&mut out);
    out.push_str("\n-- E · adversarial (non-goals, negative evidence) --\n");
    ok &= block_e(&mut out);

    // People faction type constant is part of the contract (M27 boundary).
    let _ = PEOPLE_FACTION_TYPE;

    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok { 0 } else { 1 }
}
