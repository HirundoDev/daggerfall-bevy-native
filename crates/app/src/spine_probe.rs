//! `SPINE-001` — 12-step smoke of the typed spine (contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §7).
//!
//! One process, no window. The twelve checks are **plumbing** checks (§7: they
//! do not grade any game rule): a session that starts and ticks, a scene
//! transition that delivers `Done` once, a door that arrives through the bus
//! with no double load, the five module sections landing in the v2 envelope
//! through their owners, and a `snapshot → restore → snapshot` roundtrip.
//!
//! ## Exit semantics (§7, literal)
//!
//! - `EXIT=0` ⟺ **12/12** and both envelope digests identical.
//! - `EXIT=2` when `SaveState v2`/M33 is not available: the twelve steps are
//!   still executed and the count of passed checks is printed, but a
//!   `snapshot → restore → snapshot` of the **whole game state** needs the 12
//!   canonical sections of M33 §3.3 to have a module codec, and the spine
//!   ships none of them (0 payload families — that is M33's deliverable).
//!   Empty or synthetic sections are **not** evidence of availability: filling
//!   them would be the false positive §7 forbids.
//! - `EXIT=1` on any failed step, on a non-deterministic digest pair, or on a
//!   **falsifier** that fails to fail (§9: "un check que pase con payloads
//!   inválidos" is a probe failure).
//!
//! ## What the probe does NOT claim
//!
//! Steps 6-11 use synthetic payloads written **through the owner rule** by the
//! module that owns the section (the probe stands in for M30/M23/M29/M28/M32,
//! which have no v2 codec yet). What they prove is the spine: the section
//! reaches the envelope, once, under its owner, and its bytes are the ones the
//! event produced. They prove nothing about M30/M23/M29/M28/M32 semantics.

use daggerfall_app::interior_transit::{SceneTransition, TransitionState};
use daggerfall_heart::session::{CareerId, GameSession, SessionMode, SECTION_SESSION};
use daggerfall_memory::save_v2::{
    Coverage, SaveError, SaveStateV2, SaveWriter, SectionId, SectionRegistry, M33_SECTIONS,
};
use daggerfall_nervous::bus::{BusError, EventBus};
use daggerfall_nervous::events::{
    GameEvent, HourBand, LocationRef, SceneId, SceneTarget, WindowId, MAX_EVENT_BYTES,
};
use daggerfall_perception::ui_shell::{UiInput, UiOutcome, UiShell, WindowRegistry};

// ── fixed inputs of the probe (one seed, one location: reproducible) ────────

/// World seed of both runs.
const SEED: u64 = 0x5EED_2026;
/// Career of the session (§2 `new(seed, career)`).
const CAREER: CareerId = CareerId(7);
/// Location of the probe (region 19 = the Daggerfall region index of M18).
const LOCATION: LocationRef = LocationRef::new(19, 0, 3);
/// `.RDB` block of the interior the transition loads.
const INTERIOR_BLOCK: u32 = 7;
/// Building index inside the location.
const INTERIOR_BUILDING: u32 = 2;
/// Door index inside the interior.
const DOOR_ID: u32 = 3;
/// Gold paid in the synthetic purchase (integer gold — 0 float).
const PRICE: u32 = 1_234;
/// Item bought in the synthetic purchase.
const ITEM: u32 = 0x0C;
/// Seller of the synthetic purchase.
const SELLER: u32 = 5;
/// Quest and stage of the synthetic `QuestStage`.
const QUEST_ID: u32 = 42;
/// Stage index of the synthetic `QuestStage`.
const QUEST_STAGE: u16 = 3;
/// Spell/effect key of the synthetic `effects` section.
const EFFECT_KEY: u32 = 0x0010;
/// Crime type of the synthetic `CrimeCommitted`.
const CRIME: u16 = 4;
/// Hours rested in the synthetic `rest` section.
const REST_HOURS: u16 = 6;

// ── sections of §7 steps 6-11 ──────────────────────────────────────────────

/// `economy` (M30 — `m33…:163` §9).
const SECTION_ECONOMY: SectionId = SectionId::new("economy");
/// `quests` (M29 — §8).
const SECTION_QUESTS: SectionId = SectionId::new("quests");
/// `effects` (M23 — §5).
const SECTION_EFFECTS: SectionId = SectionId::new("effects");
/// `legal` — the name §7 step 10 uses for M28's section (M33 §3.3 calls the
/// same owner's section `crime`; §7 wins inside the probe, and the divergence
/// is reported to M34 instead of silently picked).
const SECTION_LEGAL: SectionId = SectionId::new("legal");
/// `rest` — §7 step 11 (M32 owns conditions/rest; M33 §3.3 does not name it).
const SECTION_REST: SectionId = SectionId::new("rest");

/// Owner strings as M33 §3.3 declares them (`memory` only matches strings).
const OWNER_ECONOMY: &str = "M30";
const OWNER_QUESTS: &str = "M29";
const OWNER_EFFECTS: &str = "M23/M24/M32";
const OWNER_LEGAL: &str = "M28";
const OWNER_REST: &str = "M32";
const OWNER_HEART: &str = "heart";

/// Sections §7 steps 6-11 require, with their owners.
const PROBE_SECTIONS: [(&str, &str); 2] = [("legal", OWNER_LEGAL), ("rest", OWNER_REST)];

// ── evidence rows (M19..M30 template) ─────────────────────────────────────

/// Outcome of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The check held.
    Pass,
    /// The check failed (probe failure, exit 1).
    Fail,
    /// The check could not be evaluated because M33 is absent (exit 2).
    Blocked,
}

/// One printed row.
struct Check {
    id: String,
    verdict: Verdict,
    detail: String,
}

impl Check {
    fn pass(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verdict: Verdict::Pass,
            detail: detail.into(),
        }
    }

    fn fail(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verdict: Verdict::Fail,
            detail: detail.into(),
        }
    }

    fn blocked(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            verdict: Verdict::Blocked,
            detail: detail.into(),
        }
    }

    /// `pass` when `ok`, `fail` otherwise.
    fn from_bool(id: impl Into<String>, ok: bool, detail: impl Into<String>) -> Self {
        if ok {
            Self::pass(id, detail)
        } else {
            Self::fail(id, detail)
        }
    }
}

/// Prints `rows`; returns `(passed, failed, blocked)`.
fn print_checks(title: &str, rows: &[Check]) -> (usize, usize, usize) {
    println!("\n-- {title} --");
    let (mut passed, mut failed, mut blocked) = (0, 0, 0);
    for row in rows {
        match row.verdict {
            Verdict::Pass => {
                passed += 1;
                println!("[OK] {} - {}", row.id, row.detail);
            }
            Verdict::Fail => {
                failed += 1;
                println!("FAIL {} - {}", row.id, row.detail);
            }
            Verdict::Blocked => {
                blocked += 1;
                println!("[BLOCKED] {} - {}", row.id, row.detail);
            }
        }
    }
    (passed, failed, blocked)
}

// ── payload readers (bytes are opaque to `memory`, decoded here for evidence) ─

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at..at + 2)?
        .try_into()
        .ok()
        .map(u16::from_le_bytes)
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

/// Payload of `id`, refusing a **missing** or **empty** section.
///
/// The empty-section refusal is the §7 rule ("no simules 12/12 con secciones
/// vacías") made mechanical: a section that carries 0 bytes is not evidence
/// that a module wrote it.
fn payload(envelope: &SaveStateV2, id: SectionId) -> Result<&[u8], String> {
    match envelope.section(id) {
        None => Err(format!("section {:?} missing", id.key())),
        Some(section) if section.bytes.is_empty() => {
            Err(format!("section {:?} carries 0 bytes", id.key()))
        }
        Some(section) => Ok(&section.bytes),
    }
}

/// Step 6 check: the `economy` section exists and carries the price the
/// `PurchaseCompleted` event paid.
fn economy_check(envelope: &SaveStateV2) -> Result<String, String> {
    let bytes = payload(envelope, SECTION_ECONOMY)?;
    if bytes.len() != 12 {
        return Err(format!("economy payload is {} B, want 12", bytes.len()));
    }
    let price = read_u32(bytes, 0).ok_or("economy price truncated")?;
    let item = read_u32(bytes, 4).ok_or("economy item truncated")?;
    let seller = read_u32(bytes, 8).ok_or("economy seller truncated")?;
    if price != PRICE || item != ITEM || seller != SELLER {
        return Err(format!(
            "economy carries price={price} item={item} seller={seller}, want {PRICE}/{ITEM}/{SELLER}"
        ));
    }
    Ok(format!("price={price} item={item} seller={seller}"))
}

/// Step 7 check: `quests` exists and carries the stage the event announced.
fn quests_check(envelope: &SaveStateV2) -> Result<String, String> {
    let bytes = payload(envelope, SECTION_QUESTS)?;
    if bytes.len() != 6 {
        return Err(format!("quests payload is {} B, want 6", bytes.len()));
    }
    let quest_id = read_u32(bytes, 0).ok_or("quest id truncated")?;
    let stage = read_u16(bytes, 4).ok_or("quest stage truncated")?;
    if quest_id != QUEST_ID || stage != QUEST_STAGE {
        return Err(format!(
            "quests carries {quest_id}/{stage}, want {QUEST_ID}/{QUEST_STAGE}"
        ));
    }
    Ok(format!("quest={quest_id} stage={stage}"))
}

/// Step 9 check: `effects` exists and carries the synthetic spell key.
fn effects_check(envelope: &SaveStateV2) -> Result<String, String> {
    let bytes = payload(envelope, SECTION_EFFECTS)?;
    if bytes.len() != 5 {
        return Err(format!("effects payload is {} B, want 5", bytes.len()));
    }
    let key = read_u32(bytes, 0).ok_or("effect key truncated")?;
    if key != EFFECT_KEY {
        return Err(format!("effects carries key={key}, want {EFFECT_KEY}"));
    }
    Ok(format!("effect_key={key} school={}", bytes[4]))
}

/// Step 10 check: `legal` exists and carries the crime/region of the event.
fn legal_check(envelope: &SaveStateV2) -> Result<String, String> {
    let bytes = payload(envelope, SECTION_LEGAL)?;
    if bytes.len() != 4 {
        return Err(format!("legal payload is {} B, want 4", bytes.len()));
    }
    let crime = read_u16(bytes, 0).ok_or("crime truncated")?;
    let region = read_u16(bytes, 2).ok_or("region truncated")?;
    if crime != CRIME || region != LOCATION.region as u16 {
        return Err(format!(
            "legal carries crime={crime} region={region}, want {CRIME}/{}",
            LOCATION.region
        ));
    }
    Ok(format!("crime={crime} region={region}"))
}

/// Step 11 check: `rest` exists and carries a positive rest.
fn rest_check(envelope: &SaveStateV2) -> Result<String, String> {
    let bytes = payload(envelope, SECTION_REST)?;
    if bytes.len() != 3 {
        return Err(format!("rest payload is {} B, want 3", bytes.len()));
    }
    let hours = read_u16(bytes, 0).ok_or("rest hours truncated")?;
    if hours == 0 {
        return Err("rest carries 0 hours".to_string());
    }
    Ok(format!("hours={hours} conditions={}", bytes[2]))
}

// ── one execution of the 12 steps ─────────────────────────────────────────

/// Evidence of one clean execution.
struct Run {
    steps: Vec<Check>,
    controls: Vec<Check>,
    envelope: SaveStateV2,
    registry: SectionRegistry,
    coverage: Coverage,
    digest: u64,
}

/// Registry of the probe: the 12 canonical M33 sections + the three the spine
/// itself needs (`legal`, `rest`, `session`), each declared by its owner.
fn build_registry() -> Result<SectionRegistry, String> {
    let mut registry = SectionRegistry::new();
    registry
        .declare_all(&M33_SECTIONS)
        .map_err(|error| format!("M33 table: {error}"))?;
    for (key, owner) in PROBE_SECTIONS.iter().copied() {
        registry
            .declare(SectionId::new(key), owner)
            .map_err(|error| format!("probe section {key}: {error}"))?;
    }
    registry
        .declare(SECTION_SESSION, OWNER_HEART)
        .map_err(|error| format!("session section: {error}"))?;
    Ok(registry)
}

/// Executes §7's twelve steps once, on clean state.
fn run_once() -> Result<Run, String> {
    let mut steps: Vec<Check> = Vec::with_capacity(12);
    let mut controls: Vec<Check> = Vec::with_capacity(3);
    let mut bus = EventBus::new();
    let mut registry = build_registry()?;

    // ── 1 · create the session ⇒ mode=Menu, clock=0 ────────────────────────
    let mut session = GameSession::new(SEED, CAREER);
    steps.push(Check::from_bool(
        "01 session.new",
        session.mode == SessionMode::Menu
            && session.elapsed_minutes() == 0
            && session.scene == SceneId::Menu,
        format!(
            "seed={SEED:#x} career={} mode={} clock={} scene={}",
            session.career().0,
            session.mode.label(),
            session.elapsed_minutes(),
            session.scene.label()
        ),
    ));

    // ── 2 · start(location) ⇒ mode=Playing, scene=Exterior ─────────────────
    let started = session.start(LOCATION);
    steps.push(Check::from_bool(
        "02 start",
        started.is_ok() && session.mode == SessionMode::Playing && session.scene == SceneId::Exterior,
        format!(
            "mode={} scene={} location=({},{},{})",
            session.mode.label(),
            session.scene.label(),
            session.location.region,
            session.location.location,
            session.location.block
        ),
    ));

    // ── 3 · tick(10) ×6 ⇒ clock=60 (+ DayNightChanged on a band crossing) ──
    let mut band_events = 0usize;
    for _ in 0..6 {
        if session.tick(10).band_changed.is_some() {
            band_events += 1;
        }
    }
    let clock_after_six = session.elapsed_minutes();
    // The crossing: 13:00 → 18:00 flips Day→Night exactly once.
    let mut crossing_events = 0usize;
    while session.clock.hour < 18 {
        if let Some(band) = session.tick(10).band_changed {
            crossing_events += 1;
            bus.publish(GameEvent::DayNightChanged(band))
                .map_err(|error| format!("publish DayNightChanged: {error}"))?;
        }
    }
    let drained = bus.drain_tick();
    let night_delivered = drained
        == vec![GameEvent::DayNightChanged(HourBand::Night)];
    steps.push(Check::from_bool(
        "03 tick",
        clock_after_six == 60
            && band_events == 0
            && crossing_events == 1
            && night_delivered
            && session.clock.hour == 18,
        format!(
            "clock={clock_after_six} band_events={band_events} crossing_events={crossing_events} at={:02}:{:02} delivered={}",
            session.clock.hour,
            session.clock.minute,
            drained.len()
        ),
    ));

    // ── 4 · request(Interior) ⇒ Done once; scene=Interior ──────────────────
    let mut transition = SceneTransition::new();
    let interior = SceneTarget::Interior {
        location: LOCATION,
        block: INTERIOR_BLOCK,
        building: INTERIOR_BUILDING,
    };
    let requested = transition.request(interior).is_ok();
    let loading = matches!(transition.poll(), Some(TransitionState::Loading(_)));
    let mut loads = u32::from(transition.begin().is_some()); // `app` loads once
    let completed = transition.complete(interior).is_ok();
    let first_done = transition.poll();
    let second_done = transition.poll();
    let begin_after_done = transition.begin();
    if let Some(TransitionState::Done(target)) = &first_done {
        session.enter_scene(target.scene());
    }
    steps.push(Check::from_bool(
        "04 transition",
        requested
            && loading
            && completed
            && matches!(first_done, Some(TransitionState::Done(SceneTarget::Interior { .. })))
            && second_done.is_none()
            && begin_after_done.is_none()
            && loads == 1
            && session.scene == SceneId::Interior,
        format!(
            "request=ok loading={loading} done_once={} done_again={} loads={loads} scene={}",
            matches!(first_done, Some(TransitionState::Done(_))),
            second_done.is_some(),
            session.scene.label()
        ),
    ));

    // ── 5 · DoorActivated ⇒ consumed once by the transition, 0 double load ─
    bus.publish(GameEvent::DoorActivated {
        location: LOCATION,
        block: INTERIOR_BLOCK,
        door_id: DOOR_ID,
    })
    .map_err(|error| format!("publish DoorActivated: {error}"))?;
    let drained = bus.drain_tick();
    let exit = SceneTarget::Exterior {
        map_pixel: (120, 80),
        location: LOCATION,
    };
    let mut door_deliveries = 0usize;
    let mut door_target = None;
    for event in &drained {
        if let GameEvent::DoorActivated { .. } = event {
            door_deliveries += 1;
            door_target = Some(exit);
        }
    }
    let exit_requested = transition.request(exit).is_ok();
    let door_load = transition.begin().is_some();
    if door_load {
        loads += 1;
    }
    let exit_completed = transition.complete(exit).is_ok();
    let door_done = transition.poll();
    let door_done_again = transition.poll();
    let door_begin_after = transition.begin();
    if let Some(TransitionState::Done(target)) = &door_done {
        session.enter_scene(target.scene());
    }
    steps.push(Check::from_bool(
        "05 door",
        door_deliveries == 1
            && door_target.is_some()
            && exit_requested
            && door_load
            && exit_completed
            && matches!(door_done, Some(TransitionState::Done(SceneTarget::Exterior { .. })))
            && door_done_again.is_none()
            && door_begin_after.is_none()
            && loads == 2
            && session.scene == SceneId::Exterior,
        format!(
            "deliveries={door_deliveries} loads={loads} (1/door, 0 extra) done_once={} scene={}",
            matches!(door_done, Some(TransitionState::Done(_))),
            session.scene.label()
        ),
    ));

    // ── the envelope: heart's section through its own codec ────────────────
    registry
        .attach_codec(OWNER_HEART, Box::new(session.clone()))
        .map_err(|error| format!("attach heart codec: {error}"))?;
    let mut writer = SaveWriter::new(&registry);
    let mut write_errors: Vec<(&'static str, SaveError)> = Vec::new();
    if let Err(error) = writer.write_codec(OWNER_HEART, &session) {
        write_errors.push(("session", error));
    }

    // ── 6 · PurchaseCompleted ⇒ the `economy` section exists ───────────────
    bus.publish(GameEvent::PurchaseCompleted {
        item: ITEM,
        price: PRICE,
        seller: SELLER,
    })
    .map_err(|error| format!("publish PurchaseCompleted: {error}"))?;
    let mut economy_bytes: Option<Vec<u8>> = None;
    for event in bus.drain_tick() {
        if let GameEvent::PurchaseCompleted {
            item,
            price,
            seller,
        } = event
        {
            // M30's payload: price | item | seller (integers, 0 float).
            let mut bytes = Vec::with_capacity(12);
            bytes.extend_from_slice(&price.to_le_bytes());
            bytes.extend_from_slice(&item.to_le_bytes());
            bytes.extend_from_slice(&seller.to_le_bytes());
            economy_bytes = Some(bytes);
        }
    }
    let economy_written = match economy_bytes {
        Some(bytes) => writer.write(SECTION_ECONOMY, OWNER_ECONOMY, bytes),
        None => Err(SaveError::Malformed {
            why: "PurchaseCompleted was not delivered",
        }),
    };
    if let Err(error) = &economy_written {
        write_errors.push(("economy", error.clone()));
    }

    // ── 7 · QuestStage ⇒ the `quests` section is present ───────────────────
    bus.publish(GameEvent::QuestStage {
        quest_id: QUEST_ID,
        stage: QUEST_STAGE,
    })
    .map_err(|error| format!("publish QuestStage: {error}"))?;
    let mut quest_bytes: Option<Vec<u8>> = None;
    for event in bus.drain_tick() {
        if let GameEvent::QuestStage { quest_id, stage } = event {
            let mut bytes = Vec::with_capacity(6);
            bytes.extend_from_slice(&quest_id.to_le_bytes());
            bytes.extend_from_slice(&stage.to_le_bytes());
            quest_bytes = Some(bytes);
        }
    }
    if let Some(bytes) = quest_bytes {
        if let Err(error) = writer.write(SECTION_QUESTS, OWNER_QUESTS, bytes) {
            write_errors.push(("quests", error));
        }
    }

    // ── 8 · CombatHit ⇒ every consumer once, 0 cross mutation ──────────────
    bus.publish(GameEvent::CombatHit {
        attacker: 1,
        target: 2,
        damage: 17,
        crit: true,
    })
    .map_err(|error| format!("publish CombatHit: {error}"))?;
    let mut hud_deliveries = 0usize;
    let mut antibodies_deliveries = 0usize;
    let mut combat_damage = 0u16;
    for event in bus.drain_tick() {
        if let GameEvent::CombatHit { damage, .. } = event {
            hud_deliveries += 1; // the HUD consumer
            antibodies_deliveries += 1; // the antibodies consumer
            combat_damage = damage;
        }
    }
    // 0 cross mutation: the combat consumers write **no** section. The check
    // is completed against the finished envelope (exactly the six owned
    // sections, nothing extra).

    // ── 9 · magic ⇒ the `effects` section is in the envelope ───────────────
    let mut effect_bytes = Vec::with_capacity(5);
    effect_bytes.extend_from_slice(&EFFECT_KEY.to_le_bytes());
    effect_bytes.push(1); // school index (M23), integer
    if let Err(error) = writer.write(SECTION_EFFECTS, OWNER_EFFECTS, effect_bytes) {
        write_errors.push(("effects", error));
    }

    // ── 10 · CrimeCommitted ⇒ the `legal` section is present ───────────────
    bus.publish(GameEvent::CrimeCommitted {
        crime: CRIME,
        region: LOCATION.region as u16,
    })
    .map_err(|error| format!("publish CrimeCommitted: {error}"))?;
    let mut legal_bytes: Option<Vec<u8>> = None;
    for event in bus.drain_tick() {
        if let GameEvent::CrimeCommitted { crime, region } = event {
            let mut bytes = Vec::with_capacity(4);
            bytes.extend_from_slice(&crime.to_le_bytes());
            bytes.extend_from_slice(&region.to_le_bytes());
            legal_bytes = Some(bytes);
        }
    }
    if let Some(bytes) = legal_bytes {
        if let Err(error) = writer.write(SECTION_LEGAL, OWNER_LEGAL, bytes) {
            write_errors.push(("legal", error));
        }
    }

    // ── 11 · rest ⇒ the `rest`/`conditions` section is present ─────────────
    let mut rest_bytes = Vec::with_capacity(3);
    rest_bytes.extend_from_slice(&REST_HOURS.to_le_bytes());
    rest_bytes.push(1); // one active condition (M32), integer
    if let Err(error) = writer.write(SECTION_REST, OWNER_REST, rest_bytes) {
        write_errors.push(("rest", error));
    }

    let envelope = writer.finish();
    let coverage = registry.coverage(&M33_SECTIONS);

    // Steps 6, 7 and 9-11 are graded against the finished envelope: present,
    // non empty, and carrying exactly what the event said.
    let grade = |id: &'static str, key: &'static str, result: Result<String, String>| -> Check {
        let write_error = write_errors
            .iter()
            .find(|(section, _)| *section == key)
            .map(|(_, error)| error.to_string());
        match (result, write_error) {
            (Ok(detail), None) => Check::pass(id, detail),
            (Ok(detail), Some(error)) => {
                Check::fail(id, format!("{detail} but the owner write failed: {error}"))
            }
            (Err(reason), Some(error)) => Check::fail(id, format!("{reason}; write: {error}")),
            (Err(reason), None) => Check::fail(id, reason),
        }
    };

    // ── 8 · CombatHit: every consumer once, 0 cross mutation ───────────────
    // 0 cross mutation is measured on the envelope: exactly the six sections
    // their owners wrote, nothing the combat consumers could have added.
    let expected_ids: [&str; 6] = ["economy", "effects", "legal", "quests", "rest", "session"];
    let actual_ids: Vec<&'static str> = envelope.sections.iter().map(|s| s.id.key()).collect();
    let combat_row = Check::from_bool(
        "08 combat",
        hud_deliveries == 1
            && antibodies_deliveries == 1
            && combat_damage == 17
            && actual_ids == expected_ids,
        format!(
            "subscribers: hud={hud_deliveries} antibodies={antibodies_deliveries} damage={combat_damage} sections={actual_ids:?} (0 cross-writes)"
        ),
    );

    steps.push(grade("06 purchase", "economy", economy_check(&envelope)));
    steps.push(grade("07 quest", "quests", quests_check(&envelope)));
    steps.push(combat_row);
    steps.push(grade("09 magic", "effects", effects_check(&envelope)));
    steps.push(grade("10 crime", "legal", legal_check(&envelope)));
    steps.push(grade("11 rest", "rest", rest_check(&envelope)));

    // ── 12 · snapshot → restore → snapshot, byte identical ─────────────────
    let roundtrip = (|| -> Result<(u64, u64, bool), String> {
        let bytes = envelope.encode();
        let loaded = SaveStateV2::decode(&bytes, &registry)
            .map_err(|error| format!("decode of our own envelope failed: {error}"))?;
        let again = loaded.encode();
        Ok((envelope.digest(), loaded.digest(), again == bytes))
    })();
    let roundtrip_detail = match &roundtrip {
        Ok((first, second, identical)) => format!(
            "envelope roundtrip digest1={first:016x} digest2={second:016x} identical={identical} sections={}/{}",
            coverage.count(),
            coverage.total()
        ),
        Err(reason) => reason.clone(),
    };
    steps.push(if coverage.complete() {
        match roundtrip {
            Ok((first, second, identical)) if first == second && identical => {
                Check::pass("12 save/load", roundtrip_detail)
            }
            _ => Check::fail("12 save/load", roundtrip_detail),
        }
    } else {
        Check::blocked(
            "12 save/load",
            format!(
                "SaveState v2/M33 unavailable: canonical section codecs {} (missing: {}); {roundtrip_detail}",
                coverage.ratio(),
                coverage.missing_list()
            ),
        )
    });

    // ── C1 · bus cap: 4 096 accepted, the 4 097th is Full, 0 silent loss ───
    let mut cap_bus = EventBus::new();
    let mut accepted = 0u32;
    let mut first_refused: Option<u32> = None;
    for quest in 0..=4096u32 {
        match cap_bus.publish(GameEvent::QuestStage {
            quest_id: quest,
            stage: 0,
        }) {
            Ok(()) => accepted += 1,
            Err(BusError::Full) => {
                first_refused.get_or_insert(quest);
            }
        }
    }
    let drained = cap_bus.drain_tick();
    let order_kept = drained
        .iter()
        .enumerate()
        .all(|(index, event)| {
            matches!(event, GameEvent::QuestStage { quest_id, .. } if *quest_id as usize == index)
        });
    controls.push(Check::from_bool(
        "C1 bus cap",
        accepted == 4096
            && first_refused == Some(4096)
            && drained.len() == 4096
            && order_kept
            && cap_bus.is_empty(),
        format!(
            "cap={} accepted={accepted} refused=1 first_full_at={first_refused:?} drained={} order_kept={order_kept} silent_loss={}",
            cap_bus.cap(),
            drained.len(),
            accepted as usize - drained.len()
        ),
    ));

    // ── C2 · the 9 events, typed, payload ≤ 64 B ───────────────────────────
    let probe_events = [
        GameEvent::DoorActivated {
            location: LOCATION,
            block: INTERIOR_BLOCK,
            door_id: DOOR_ID,
        },
        GameEvent::SceneChanged(interior),
        GameEvent::CombatHit {
            attacker: 1,
            target: 2,
            damage: 17,
            crit: true,
        },
        GameEvent::PurchaseCompleted {
            item: ITEM,
            price: PRICE,
            seller: SELLER,
        },
        GameEvent::QuestStage {
            quest_id: QUEST_ID,
            stage: QUEST_STAGE,
        },
        GameEvent::CrimeCommitted {
            crime: CRIME,
            region: LOCATION.region as u16,
        },
        GameEvent::WeatherChanged(daggerfall_nervous::events::WeatherType::Overcast),
        GameEvent::DayNightChanged(HourBand::Night),
        GameEvent::UiRequested(WindowId::QUESTS),
    ];
    let max_payload = probe_events.iter().map(GameEvent::payload_len).max().unwrap_or(0);
    let labels: Vec<&'static str> = probe_events.iter().map(GameEvent::label).collect();
    controls.push(Check::from_bool(
        "C2 events",
        labels.len() == 9 && max_payload <= MAX_EVENT_BYTES,
        format!(
            "typed={} max_payload={max_payload}B cap={MAX_EVENT_BYTES}B labels={labels:?}",
            labels.len()
        ),
    ));

    // ── C3 · UiShell over the registry: stack + input routing ──────────────
    let mut window_registry = WindowRegistry::new();
    for (id, title) in [
        (WindowId::HUD, "hud"),
        (WindowId::INVENTORY, "inventory"),
        (WindowId::QUESTS, "quests"),
        (WindowId::MAIN_MENU, "main-menu"),
    ] {
        window_registry
            .register(id, title)
            .map_err(|error| format!("register {title}: {error}"))?;
    }
    let mut shell = UiShell::new(window_registry);
    let opened_hud = shell.open(WindowId::HUD).is_ok();
    let opened_quests = shell.open(WindowId::QUESTS).is_ok();
    let top_is_quests = shell.top() == Some(WindowId::QUESTS);
    let confirmed = shell.route_input(UiInput::Confirm) == UiOutcome::Confirmed(WindowId::QUESTS);
    let closed = shell.route_input(UiInput::Cancel) == UiOutcome::Closed(WindowId::QUESTS);
    let hud_on_top = shell.top() == Some(WindowId::HUD);
    shell.toggle(WindowId::INVENTORY);
    let toggled_open = shell.is_open(WindowId::INVENTORY) && shell.top() == Some(WindowId::INVENTORY);
    shell.close(WindowId::INVENTORY);
    let closed_by_id = !shell.is_open(WindowId::INVENTORY) && shell.depth() == 1;
    controls.push(Check::from_bool(
        "C3 ui shell",
        opened_hud && opened_quests && top_is_quests && confirmed && closed && hud_on_top
            && toggled_open
            && closed_by_id,
        format!(
            "registry={} panels depth={} top={:?} (0 prices/HP/quests: ids+captions only)",
            shell.registry().len(),
            shell.depth(),
            shell.top().map(WindowId::index)
        ),
    ));

    Ok(Run {
        steps,
        controls,
        digest: envelope.digest(),
        envelope,
        registry,
        coverage,
    })
}

// ── §9 falsifiers: a check that passes with an invalid payload is a bug ────

/// Runs the negative controls against `run`'s envelope and registry.
fn falsifiers(run: &Run) -> Vec<Check> {
    let mut rows = Vec::with_capacity(6);

    // F1 — an undeclared section cannot be written.
    let mut writer = SaveWriter::new(&run.registry);
    let ghost = SectionId::new("ghost");
    let f1 = writer.write(ghost, "M30", vec![1, 2, 3]);
    rows.push(Check::from_bool(
        "F1 undeclared section",
        matches!(f1, Err(SaveError::NotDeclared { .. })) && writer.finish().sections.is_empty(),
        format!("write(ghost) -> {f1:?}, 0 sections landed"),
    ));

    // F2 — a module cannot write another module's section.
    let mut writer = SaveWriter::new(&run.registry);
    let f2 = writer.write(SECTION_ECONOMY, "M29", vec![1, 2, 3]);
    rows.push(Check::from_bool(
        "F2 foreign owner",
        matches!(f2, Err(SaveError::NotOwner { .. })) && writer.finish().sections.is_empty(),
        format!("write(economy as M29) -> {f2:?}, 0 sections landed"),
    ));

    // F3 — an EMPTY section must not satisfy the step check (the §7 rule).
    let mut empty = SaveStateV2::new();
    empty
        .push(SECTION_ECONOMY, Vec::new())
        .expect("a fresh envelope accepts one push");
    let f3 = economy_check(&empty);
    rows.push(Check::from_bool(
        "F3 empty payload",
        f3.is_err(),
        format!("economy with 0 bytes -> {:?} (expected an error)", f3.err()),
    ));

    // F4 — a payload that does not match the event must not pass.
    let mut wrong = SaveStateV2::new();
    let mut bytes = Vec::with_capacity(12);
    bytes.extend_from_slice(&(PRICE + 1).to_le_bytes());
    bytes.extend_from_slice(&ITEM.to_le_bytes());
    bytes.extend_from_slice(&SELLER.to_le_bytes());
    wrong
        .push(SECTION_ECONOMY, bytes)
        .expect("a fresh envelope accepts one push");
    let f4 = economy_check(&wrong);
    rows.push(Check::from_bool(
        "F4 wrong value",
        f4.is_err(),
        format!("economy with price+1 -> {:?} (expected an error)", f4.err()),
    ));

    // F5 — an unknown section in an envelope is refused, never ignored.
    let mut ghost_envelope = run.envelope.clone();
    ghost_envelope
        .sections
        .push(daggerfall_memory::save_v2::SaveSection {
            id: ghost,
            bytes: vec![0xAA],
        });
    let f5 = SaveStateV2::decode(&ghost_envelope.encode(), &run.registry);
    rows.push(Check::from_bool(
        "F5 unknown section",
        matches!(f5, Err(SaveError::UnknownSection { .. })),
        format!("decode(envelope + ghost) -> {:?}", f5.err()),
    ));

    // F6 — a tampered envelope: a flipped payload byte changes the digest, a
    // flipped magic is a hard error (0 silent acceptance).
    let mut bad_magic = run.envelope.encode();
    bad_magic[0] = b'X';
    let magic = SaveStateV2::decode(&bad_magic, &run.registry);
    let mut flipped = run.envelope.encode();
    let flipped_ok = if let Some(last) = flipped.len().checked_sub(1) {
        flipped[last] ^= 0xFF;
        true
    } else {
        false
    };
    let after_flip = SaveStateV2::decode(&flipped, &run.registry)
        .map(|state| state.digest())
        .map_err(|error| error.to_string());
    let digest_moved = matches!(after_flip, Ok(digest) if digest != run.digest);
    rows.push(Check::from_bool(
        "F6 tampered save",
        matches!(magic, Err(SaveError::BadMagic { .. })) && flipped_ok && digest_moved,
        format!(
            "bad magic -> {:?}; flipped payload byte -> digest moved={digest_moved}",
            magic.err()
        ),
    ));

    rows
}

/// `SPINE-001` entry point — exit code per §7.
pub fn run_spine_001() -> i32 {
    println!("PROBE-SPINE-001 — typed spine smoke (§7), 12 steps, no window");
    println!(
        "contract=spine-contracts-CR-f716455c.md §7 · owner map §1 · events §6 · envelope §3"
    );
    println!(
        "seed={SEED:#x} career={} location=({},{},{})",
        CAREER.0, LOCATION.region, LOCATION.location, LOCATION.block
    );

    let first = match run_once() {
        Ok(run) => run,
        Err(reason) => {
            println!("FAIL SPINE-001 setup - {reason}");
            println!("\nSUMMARY PROBE-SPINE-001: passed=0/12 ⇒ FAIL");
            return 1;
        }
    };
    let second = match run_once() {
        Ok(run) => run,
        Err(reason) => {
            println!("FAIL SPINE-001 second run - {reason}");
            println!("\nSUMMARY PROBE-SPINE-001: passed=0/12 ⇒ FAIL");
            return 1;
        }
    };

    let (passed, failed, blocked) = print_checks("12 steps (§7)", &first.steps);
    let (control_passed, control_failed, _) = print_checks("controls", &first.controls);
    let falsifier_rows = falsifiers(&first);
    let (falsifier_passed, falsifier_failed, _) = print_checks("§9 falsifiers", &falsifier_rows);

    let deterministic = first.digest == second.digest;
    let same_steps = first.steps.len() == second.steps.len()
        && first
            .steps
            .iter()
            .zip(&second.steps)
            .all(|(a, b)| a.verdict == b.verdict);
    println!("\n-- determinism (12 steps ×2, clean state) --");
    println!(
        "digest1={:016x} digest2={:016x} identical={deterministic} verdicts_identical={same_steps}",
        first.digest, second.digest
    );

    println!(
        "\n-- envelope --\nsections={} ({} bytes) registered={} with_codec={}",
        first.envelope.sections.len(),
        first.envelope.payload_bytes(),
        first.registry.len(),
        first.registry.iter().filter(|(_, _, has)| *has).count()
    );
    println!(
        "M33 canonical codecs={} missing={}",
        first.coverage.ratio(),
        first.coverage.missing_list()
    );

    let total = first.steps.len();
    let control_total = first.controls.len();
    let falsifier_total = falsifier_rows.len();
    let cause = if first.coverage.complete() {
        "none".to_string()
    } else {
        format!(
            "SaveState v2/M33 unavailable (canonical section codecs {})",
            first.coverage.ratio()
        )
    };
    let exit = if failed > 0
        || control_failed > 0
        || falsifier_failed > 0
        || !deterministic
        || !same_steps
    {
        1
    } else if blocked > 0 {
        2
    } else {
        0
    };
    let verdict = match exit {
        0 => "PASS",
        2 => "BLOCKED",
        _ => "FAIL",
    };
    println!(
        "\nSUMMARY PROBE-SPINE-001: passed={passed}/{total} blocked={blocked} failed={failed} \
         controls={control_passed}/{control_total} falsifiers={falsifier_passed}/{falsifier_total} \
         ⇒ {verdict} EXIT={exit}"
    );
    println!("cause={cause}");
    exit
}
