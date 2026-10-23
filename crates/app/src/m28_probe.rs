//! M28 probes — crime, law and courts (SPEC-M28-CRIME-LAW §5,
//! REQ-L99-061).
//!
//! `PROBE-CRIME-001` — tables and registration (pure, 0 retail bytes):
//! - **A** GV-28-01..12: `penaltyAmount` after the clamps and its `/ 40`.
//! - **B** GV-28-13..16: legal reputation and the People faction propagation
//!   through M27's `FactionGraph::change_reputation` (ally/enemy visible).
//! - **C** determinism: sentence ×2 with `DFRandom::srand(12345)`, identical
//!   `(fine, days)` and digest; the split identity of `:148-165`.
//! - **D** censo §1.3: `triggers=7`, `loan_default_without_guards=true`, the
//!   hourly conspiracy and the `setplayercrime` validation.
//!
//! `PROBE-LAW-001` — crime → guards → court → prison → release (§5.2):
//! step 1 crime + guard request (M26 contract: threshold 2, quota 4),
//! step 2 guard hit lowers the legal reputation once,
//! step 3 voluntary surrender opens the court (and the punishment type is
//! measured: P(type 2) with `LegalRep=-50`),
//! step 4 sentence with the Murder row (`⌊10500/40⌋ = 262` steps) compared
//! against an independent recomputation of the split loop,
//! step 5 prison clock `days * 86400`,
//! step 6 release: `CrimeCommitted = None`, `+14400 s`, enemies cleared.
//!
//! Anti-false-positive (§5.2): a `Crimes::None` court consumes no table, a
//! `LegalRep` written as -101 clamps to -100 (and the unclamped amount differs),
//! and `LegalRep=-100` yields punishment type 0 at least once in 20 seeded
//! draws (the type is not constant).
//!
//! **Custody (INV-L99-6):** 0 retail bytes — both probes are pure and never
//! touch ARENA2.

use daggerfall_antibodies::crime_guardrails::{check_prison_clock, check_sentence, LawViolation};
use daggerfall_behavior::factions::FactionGraph;
use daggerfall_behavior::law::{
    finish_sentence, guard_hit_response, guard_spawn_request, hourly_crime, loan_default_crime,
    penalty_amount, punishment_type, quest_crime, rescue_for_crime, sentence, sentence_digest,
    surrender_to_city_guards, CourtSession, CourtState, Crimes, DfRandom, GuildRescue,
    GuardSpawnContext, PlayerLaw, PrisonState, RegionLaw, CRIME_TRIGGERS, FINE_STEP,
    LOAN_DEFAULT_SPAWNS_GUARDS, MIN_LEGAL_REP, PUNISHMENT_FINE_OR_PRISON, PUNISHMENT_SEVERE,
    SEVERE_PUNISHMENT_BANISHED,
};
use daggerfall_parser_bevy_bridge::npc_spec::{GUARDS_PER_SPAWN, GUARD_CRIME_THRESHOLD};

// ── evidence rows (M19/M25/M26/M27 template) ───────────────────────────────

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

/// Region 0 People faction (`type: 15`) with one ally and one enemy: the
/// propagation of M27 is observable on both sides.
const PEOPLE_GRAPH: &str = "#77\ntype: 15\nregion: 1\nname: People\nally: 78\nenemy: 79\n#78\ntype: 2\nname: Ally\n#79\ntype: 2\nname: Enemy\n";

fn people_graph() -> FactionGraph {
    FactionGraph::parse(PEOPLE_GRAPH, 0x1234).expect("valid fixture")
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-CRIME-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-CRIME-001` — court table and crime registration.
pub fn run_crime_001() -> i32 {
    println!(
        "PROBE-CRIME-001 - crime: table, reputation, determinism, triggers (M28, CR-5243af63, REQ-L99-061 · spec §5.1)"
    );

    // ── A · GV-28-01..12 ──────────────────────────────────────────────────
    let cases: [(&str, Crimes, i16, i32, i32); 12] = [
        ("A1 GV-28-01 Trespassing", Crimes::Trespassing, 0, 200, 5),
        ("A2 GV-28-02 Assault", Crimes::Assault, 0, 1000, 25),
        ("A3 GV-28-03 Murder", Crimes::Murder, 0, 10000, 250),
        ("A4 GV-28-04 Murder -100", Crimes::Murder, -100, 11000, 275),
        ("A5 GV-28-05 Murder +100", Crimes::Murder, 100, 11000, 275),
        ("A6 GV-28-06 HighTreason +100", Crimes::HighTreason, 100, 2000, 50),
        ("A7 GV-28-07 HighTreason -100", Crimes::HighTreason, -100, 2000, 50),
        ("A8 GV-28-08 Theft -50", Crimes::Theft, -50, 200, 5),
        ("A9 GV-28-09 LoanDefault 0", Crimes::LoanDefault, 0, 100, 2),
        ("A10 GV-28-10 Vagrancy 0", Crimes::Vagrancy, 0, 100, 2),
        ("A11 GV-28-11 Pickpocketing +30", Crimes::Pickpocketing, 30, 380, 9),
        ("A12 GV-28-12 Smuggling -100", Crimes::Smuggling, -100, 1300, 32),
    ];
    let mut rows = Vec::with_capacity(cases.len());
    for (id, crime, rep, want_amount, want_steps) in cases {
        let amount = penalty_amount(rep, crime).expect("court table row");
        let steps = amount / FINE_STEP;
        println!(
            "{}: amount={} iterations={} legal_rep={}",
            crime.name(),
            amount,
            steps,
            rep
        );
        rows.push(row(
            id,
            format!("amount={amount} iterations={steps}"),
            format!("amount={want_amount} iterations={want_steps}"),
            format!("{} at LegalRep={rep}", crime.name()),
        ));
    }
    let mut all_ok = print_rows("A · court table (GV-28-01..12)", &rows);

    // ── B · GV-28-13..16 ──────────────────────────────────────────────────
    let mut rows = Vec::new();
    let mut graph = people_graph();
    let mut law = PlayerLaw::new(1);
    let delta = law
        .lower_rep_for_crime(0, Crimes::Murder, &mut graph)
        .expect("region 0");
    let b1 = format!(
        "legal_after={} people_after={} propagated_allies={} propagated_enemies={}",
        delta.legal_rep_after,
        graph.reputation(77),
        graph.reputation(78),
        graph.reputation(79)
    );
    println!("{b1}");
    rows.push(row(
        "B1 GV-28-13 Murder",
        b1,
        "legal_after=-20 people_after=-10 propagated_allies=-5 propagated_enemies=5",
        "loss 20; People -20/2 propagated to ally/enemy",
    ));

    let mut graph = people_graph();
    let mut law = PlayerLaw::new(1);
    let delta = law
        .lower_rep_for_crime(0, Crimes::HighTreason, &mut graph)
        .expect("region 0");
    rows.push(row(
        "B2 GV-28-14 HighTreason",
        format!(
            "legal_after={} people_after={}",
            delta.legal_rep_after,
            graph.reputation(77)
        ),
        "legal_after=-75 people_after=-37",
        "loss 75; People -(75/2)",
    ));

    let mut graph = people_graph();
    let mut law = PlayerLaw::new(1);
    let delta = law
        .lower_rep_for_crime(0, Crimes::Vagrancy, &mut graph)
        .expect("region 0");
    rows.push(row(
        "B3 GV-28-15 Vagrancy",
        format!(
            "legal_after={} people_after={}",
            delta.legal_rep_after,
            graph.reputation(77)
        ),
        "legal_after=-1 people_after=0",
        "loss 1; People -(1/2) = 0",
    ));

    let mut graph = people_graph();
    let mut law = PlayerLaw::new(1);
    law.set_crime(Crimes::Murder, false);
    law.lower_rep_for_crime(0, Crimes::Murder, &mut graph)
        .expect("region 0");
    let before = law.region(0).expect("region").legal_rep;
    law.open_court(0, 1_000_000, GuildRescue::default(), &mut DfRandom::srand(12345))
        .expect("court");
    let delta = law.raise_rep_for_sentence(0, &mut graph).expect("region 0");
    rows.push(row(
        "B4 GV-28-16 sentence",
        format!(
            "half={} raised={} people_after={}",
            law.half_of_legal_rep_lost_from_crime,
            delta.legal_rep_after - before,
            graph.reputation(77)
        ),
        "half=10 raised=9 people_after=-6",
        "RaiseReputationForDoingSentence: += half-1; People += (half-1)/2",
    ));
    all_ok &= print_rows("B · legal reputation (GV-28-13..16)", &rows);

    // ── C · determinism ───────────────────────────────────────────────────
    let first = sentence(-50, Crimes::Murder, 0, &mut DfRandom::srand(12345)).expect("row");
    let second = sentence(-50, Crimes::Murder, 0, &mut DfRandom::srand(12345)).expect("row");
    let digest_first = sentence_digest(first.fine, first.days_in_prison);
    let digest_second = sentence_digest(second.fine, second.days_in_prison);
    let c_line = format!(
        "seed=12345 steps={} fine={} days={} digest={:016x}",
        first.steps, first.fine, first.days_in_prison, digest_first
    );
    println!("{c_line}");
    let mut rows = vec![
        row(
            "C1 determinism x2",
            format!("{:?}", (first.fine, first.days_in_prison)),
            format!("{:?}", (second.fine, second.days_in_prison)),
            "same DFRandom sequence ⇒ identical sentence",
        ),
        check(
            "C2 digest identical",
            digest_first == digest_second,
            format!("digest1={digest_first:016x} digest2={digest_second:016x}"),
        ),
        check(
            "C3 split identity",
            {
                let uncapped =
                    sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(12345))
                        .expect("row");
                uncapped.fine / FINE_STEP + uncapped.days_in_prison / 3 == uncapped.steps
                    && uncapped.steps == 262
            },
            "⌊10500/40⌋ = 262 iterations; fine/40 + days/3 = iterations before halving",
        ),
        check(
            "C4 antibodies shape",
            check_sentence(
                first.steps,
                first.fine,
                first.days_in_prison,
                0,
                Crimes::Murder.value(),
            )
            .is_ok(),
            "fine ≤ gold, crime valid, no table for None",
        ),
    ];
    rows.push(check(
        "C5 antibodies uncapped identity",
        check_sentence(
            second.steps,
            sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(7))
                .expect("row")
                .fine,
            sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(7))
                .expect("row")
                .days_in_prison,
            1_000_000,
            Crimes::Murder.value(),
        )
        .is_ok(),
        "identity checked only when the gold cap does not bind",
    ));
    all_ok &= print_rows("C · determinism (spec §5.1 C)", &rows);

    // ── D · triggers §1.3 ─────────────────────────────────────────────────
    let immediate = CRIME_TRIGGERS
        .iter()
        .filter(|trigger| trigger.immediate_guards())
        .count();
    let hourly = (0..64)
        .filter(|seed| {
            hourly_crime(-11, 0, &mut DfRandom::srand(*seed)) == Some(Crimes::CriminalConspiracy)
        })
        .count();
    let banned = (0..64)
        .filter(|seed| {
            hourly_crime(0, SEVERE_PUNISHMENT_BANISHED, &mut DfRandom::srand(*seed))
                == Some(Crimes::CriminalConspiracy)
        })
        .count();
    let d_line = format!(
        "triggers={} immediate={} loan_default_without_guards={} hourly_rep={}/64 hourly_banished={}/64 quest=Murder:{} bogus:{}",
        CRIME_TRIGGERS.len(),
        immediate,
        !LOAN_DEFAULT_SPAWNS_GUARDS,
        hourly,
        banned,
        quest_crime("Murder").map_or(-1, |crime| i16::from(crime.value())),
        quest_crime("Not_A_Crime").map_or(-1, |crime| i16::from(crime.value())),
    );
    println!("{d_line}");
    let rows = vec![
        row(
            "D1 trigger census",
            format!("triggers={} immediate={}", CRIME_TRIGGERS.len(), immediate),
            "triggers=7 immediate=4",
            "spec §1.3: four crimes spawn guards immediately",
        ),
        check(
            "D2 loan default has no guards",
            loan_default_crime(true, false, false) == Some(Crimes::LoanDefault)
                && !LOAN_DEFAULT_SPAWNS_GUARDS,
            "LoanChecker.cs:53-70 lowers reputation only",
        ),
        check(
            "D3 hourly conspiracy",
            hourly > 0 && hourly < 64 && banned > hourly,
            format!("rep={hourly}/64 banished={banned}/64 (SuccessRoll 5 vs 10)"),
        ),
        row(
            "D4 quest verb validation",
            format!(
                "{} {}",
                quest_crime("Murder").map_or(-1, |crime| i16::from(crime.value())),
                quest_crime("Not_A_Crime").map_or(-1, |crime| i16::from(crime.value()))
            ),
            "5 -1",
            "unknown name ⇒ caller raises (SetPlayerCrime.cs:46-51)",
        ),
    ];
    all_ok &= print_rows("D · crime sources (spec §1.3)", &rows);

    if all_ok {
        println!("\nVERDICT: PROBE-CRIME-001 PASS");
        0
    } else {
        println!("\nVERDICT: PROBE-CRIME-001 FAIL");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-LAW-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-LAW-001` — crime → guards → court → prison → release.
pub fn run_law_001() -> i32 {
    println!(
        "PROBE-LAW-001 - crime -> guards -> court -> prison -> release (M28, CR-5243af63, REQ-L99-061 · spec §5.2)"
    );
    const GOLD: i32 = 1_000_000;

    // ── step 1 · crime registered + guard request (M26 contract) ───────────
    let mut law = PlayerLaw::new(1);
    let crime = law.set_crime(Crimes::Murder, false);
    let spawn = guard_spawn_request(
        GuardSpawnContext {
            active_guards: 0,
            immediate: true,
            npcs_in_radius: 3,
            ..GuardSpawnContext::default()
        },
        &mut DfRandom::srand(1),
    );
    let spawn_requested = spawn.is_some() && crime.value() >= GUARD_CRIME_THRESHOLD;
    println!(
        "step1 crime={} guard_spawn_requested={spawn_requested} m26_threshold={GUARD_CRIME_THRESHOLD} m26_guards={GUARDS_PER_SPAWN} request={spawn:?}",
        crime.value()
    );
    let mut all_ok = print_rows(
        "step 1 · crime + guard request",
        &[
            row(
                "1a crime registered",
                format!("crime={}", crime.value()),
                "crime=5",
                "Crimes::Murder (`PlayerEntity.cs:2263-2281`)",
            ),
            check(
                "1b guard spawn requested",
                spawn_requested,
                format!(
                    "M26 spawns {GUARDS_PER_SPAWN} guards when crime ≥ {GUARD_CRIME_THRESHOLD} (npc_spec.rs:75-77)"
                ),
            ),
        ],
    );

    // ── step 2 · guard hit lowers the legal reputation once ────────────────
    *law.region_mut(0).expect("region 0") = RegionLaw {
        legal_rep: -30,
        severe_punishment_flags: 0,
    };
    let mut graph = people_graph();
    let first_hit = guard_hit_response(crime, false, true, false);
    let second_hit = guard_hit_response(crime, true, true, false);
    let delta = law
        .lower_rep_for_crime(0, Crimes::Murder, &mut graph)
        .expect("region 0");
    let dialogues = usize::from(first_hit.show_surrender_dialogue)
        + usize::from(second_hit.show_surrender_dialogue);
    println!(
        "step2 legal_rep_before=-30 after={} dialogues={dialogues} people_after={}",
        delta.legal_rep_after,
        graph.reputation(77)
    );
    all_ok &= print_rows(
        "step 2 · guard hit",
        &[
            row(
                "2a legal reputation",
                format!("-30 -> {}", delta.legal_rep_after),
                "-30 -> -50",
                "Murder loss 20 (`PlayerEntity.cs:2284`)",
            ),
            row(
                "2b surrender dialogue",
                format!("{dialogues}"),
                "1",
                "asked once per crime (`EnemyAttack.cs:270-296`)",
            ),
            row(
                "2c People faction",
                format!("{}", graph.reputation(77)),
                "-10",
                "half of the loss, propagated (M27)",
            ),
        ],
    );

    // ── step 3 · voluntary surrender opens the court ──────────────────────
    let legal_before_court = law.region(0).expect("region 0").legal_rep;
    let surrender = surrender_to_city_guards(legal_before_court, 1, true, &mut DfRandom::srand(2))
        .expect("alive");
    let court = law
        .open_court(0, GOLD, GuildRescue::default(), &mut DfRandom::srand(3))
        .expect("region 0");
    const TRIALS: u32 = 20_000;
    let type2 = (0..TRIALS)
        .filter(|seed| {
            punishment_type(-50, &mut DfRandom::srand(*seed)) == PUNISHMENT_FINE_OR_PRISON
        })
        .count();
    let probability = f64::from(type2 as u32) / f64::from(TRIALS);
    let rescued = rescue_for_crime(
        Crimes::Murder,
        GuildRescue {
            dark_brotherhood_member: true,
            dark_brotherhood_rank: 19,
            thieves_member: false,
            thieves_rank: -1,
        },
        &mut DfRandom::srand(4),
    );
    println!(
        "step3 court_open={} state={} p_type2={probability:.4} expected=0.375 rescue={rescued:?}",
        surrender.court,
        court.state.code()
    );
    all_ok &= print_rows(
        "step 3 · surrender and punishment type",
        &[
            check("3a court opened", surrender.court, "LegalRep -50 is below -20 ⇒ always court"),
            row(
                "3b court state",
                format!("{}", court.state.code()),
                "1",
                "state 1 waits for the guilty/not-guilty button",
            ),
            check(
                "3c punishment type probability",
                (probability - 0.375).abs() < 0.015,
                format!("t1=50,t2=25 ⇒ P(2)=0.75·0.5; measured {probability:.4} over {TRIALS} seeds"),
            ),
            check(
                "3d guild rescue (M27 rank)",
                rescued == Some(daggerfall_behavior::law::RescueGuild::DarkBrotherhood),
                "Dark Brotherhood rank 19 ≥ Random.Range(0,20) always rescues",
            ),
        ],
    );

    // ── step 4 · guilty sentence of the Murder row ────────────────────────
    let seed = (0..2_000)
        .find(|seed| {
            punishment_type(-50, &mut DfRandom::srand(*seed)) == PUNISHMENT_FINE_OR_PRISON
        })
        .expect("a seed with punishment type 2");
    let expected = sentence(-50, Crimes::Murder, GOLD, &mut DfRandom::srand(seed)).expect("row");
    // Independent recomputation of the split loop (`DaggerfallCourtWindow.cs:148-165`).
    let mut manual = DfRandom::srand(seed);
    let threshold2_failed = ((manual.rand() % 100) as i32) >= 25;
    let punishment = if threshold2_failed && ((manual.rand() % 100) as i32) >= 50 {
        PUNISHMENT_FINE_OR_PRISON
    } else {
        PUNISHMENT_SEVERE
    };
    let mut manual_fine = 0;
    let mut manual_days = 0;
    for _ in 0..262 {
        if manual.rand() & 1 != 0 {
            manual_fine += FINE_STEP;
        } else {
            manual_days += 3;
        }
    }
    let mut session =
        CourtSession::open(Crimes::Murder, 0, -50, GOLD, GuildRescue::default(), &mut DfRandom::srand(seed));
    let sentence_matches = session.sentence
        == Some(daggerfall_behavior::law::Sentence {
            punishment_type: punishment,
            fine: manual_fine,
            days_in_prison: manual_days,
            steps: 262,
        });
    let identity = expected.fine / FINE_STEP + expected.days_in_prison / 3 == expected.steps;
    session.answer_guilty();
    println!(
        "step4 seed={seed} punishment_type={} steps={} fine_pre={} days_pre={} fine_paid={} days={} state={} manual_match={sentence_matches} identity={identity}",
        expected.punishment_type,
        expected.steps,
        expected.fine,
        expected.days_in_prison,
        session.fine_paid,
        session.days_left,
        session.state.code()
    );
    all_ok &= print_rows(
        "step 4 · guilty sentence",
        &[
            row(
                "4a iterations",
                format!("{}", expected.steps),
                "262",
                "⌊(10000 - 10·(-50))/40⌋ = ⌊10500/40⌋",
            ),
            check(
                "4b sentence matches the independent loop",
                sentence_matches,
                format!(
                    "manual fine={manual_fine} days={manual_days} vs library fine={} days={}",
                    expected.fine, expected.days_in_prison
                ),
            ),
            check(
                "4c split identity before halving",
                identity,
                "fine/40 + days/3 = iterations (`:148-165`)",
            ),
            row(
                "4d guilty halves",
                format!("fine={} days={}", session.fine_paid, session.days_left),
                format!("fine={} days={}", expected.fine >> 1, expected.days_in_prison >> 1),
                "punishment type 2 ⇒ `fine >>= 1; daysInPrison >>= 1` (`:332-333`)",
            ),
        ],
    );

    // ── step 5 · prison ───────────────────────────────────────────────────
    let days = session.days_left;
    let mut prison = PrisonState::begin(days);
    let mut ended = false;
    for _ in 0..days {
        ended = prison.tick_day();
    }
    let served = law
        .serve_sentence(&session, &mut graph)
        .expect("state 3 raises the reputation");
    println!(
        "step5 in_prison={} days_in_prison={days} days_left={} clock_advanced={} ended={ended} legal_rep_after={}",
        law.in_prison, prison.days_left, prison.clock_advanced, served.legal_rep_after
    );
    all_ok &= print_rows(
        "step 5 · prison",
        &[
            check("5a in prison", law.in_prison, "state 3 sets `InPrison = true` (`:253-261`)"),
            check(
                "5b clock",
                check_prison_clock(days, prison.clock_advanced, false).is_ok(),
                format!("days * 1440 * 60 = {}", i64::from(days) * 86_400),
            ),
            row(
                "5c reputation raise",
                format!("{}", served.legal_rep_after),
                format!("{}", -50 + 9),
                "half=10 ⇒ += 9 (`:2301-2311`)",
            ),
        ],
    );

    // ── step 6 · release ──────────────────────────────────────────────────
    let added = finish_sentence(&mut prison, &mut law);
    let release_ok = check_prison_clock(days, prison.clock_advanced, true).is_ok();
    println!(
        "step6 crime={} clock_advanced={} release_added={added} enemies_cleared={} release_clock={release_ok}",
        law.crime.name(),
        prison.clock_advanced,
        daggerfall_behavior::law::RELEASE_CLEARS_ENEMIES
    );
    all_ok &= print_rows(
        "step 6 · release",
        &[
            row(
                "6a crime cleared",
                law.crime.name().to_string(),
                "None",
                "`CrimeCommitted = None` (`:485`)",
            ),
            check(
                "6b clock",
                release_ok,
                format!(
                    "days*86400 + 14400 = {}",
                    i64::from(days) * 86_400 + 14_400
                ),
            ),
            check(
                "6c enemies cleared request",
                daggerfall_behavior::law::RELEASE_CLEARS_ENEMIES,
                "`ClearEnemies()` at `:488` — the registry is M26/engine",
            ),
        ],
    );

    // ── anti-false-positive ───────────────────────────────────────────────
    let none_court = CourtSession::open(
        Crimes::None,
        0,
        -100,
        0,
        GuildRescue::default(),
        &mut DfRandom::srand(5),
    );
    let mut clamped = RegionLaw::new();
    let clamped_value = clamped.set_legal_rep(-101);
    let clamped_amount = penalty_amount(clamped_value, Crimes::Murder).expect("row");
    let unclamped_amount = penalty_amount(-101, Crimes::Murder).expect("row");
    let type0_in_20 = (0..20)
        .filter(|seed| punishment_type(-100, &mut DfRandom::srand(*seed)) == PUNISHMENT_SEVERE)
        .count();
    println!(
        "afp none_court={} clamped={clamped_value} clamped_amount={clamped_amount} unclamped_amount={unclamped_amount} type0_in_20_at_minus100={type0_in_20}",
        none_court.sentence.is_none()
    );
    all_ok &= print_rows(
        "anti-false-positive",
        &[
            check(
                "AFP-a None consumes no table",
                none_court.sentence.is_none() && none_court.state == CourtState::Done,
                "court closes before the table (`:109-113`)",
            ),
            check(
                "AFP-b clamp changes the amount",
                clamped_value == MIN_LEGAL_REP
                    && clamped_amount == 11_000
                    && unclamped_amount == 11_010,
                "LegalRep -101 clamps to -100 (`:2247-2256`)",
            ),
            check(
                "AFP-c type is not constant",
                type0_in_20 > 0,
                "at LegalRep=-100, type 0 appears in the 20 seeded draws",
            ),
            check(
                "AFP-d antibodies reject a None table",
                daggerfall_antibodies::crime_guardrails::check_sentence(0, 0, 0, 0, 0)
                    == Err(LawViolation::CourtTableWithoutCrime { fine: 0, days: 0 }),
                "the guardrail types the invalid court",
            ),
        ],
    );

    if all_ok {
        println!("\nVERDICT: PROBE-LAW-001 PASS");
        0
    } else {
        println!("\nVERDICT: PROBE-LAW-001 FAIL");
        1
    }
}
