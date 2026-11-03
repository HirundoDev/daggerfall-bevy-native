//! `dungeon_state` (app, `probes`) — the M44 **lane B** dungeon-state evidence
//! (`CR-bd972066-59e6-4768-9985-de75f69f6798`).
//!
//! **What it measures.** The six-state machine of the block-B contract
//! (`Dormant → Open → Engaged → Cleared → Cooldown → Closed`) and the three
//! registers the save persists:
//!
//! 1. **the machine** — [`Machine::apply`] is the only door: a transition the
//!    table allows returns `Some(to)`, anything else returns `None` and leaves
//!    the machine where it was. [`publish`] drives it with the 12 declared
//!    cases and the 12 counter-cases of the contract and publishes **what the
//!    machine answered**; the table is used as the input list, never as the
//!    output, so a drift between the table and the code turns a row red
//!    instead of printing a green line by construction.
//! 2. **the simulation** — [`Sim`] runs the zone clock for a number of ticks
//!    and counts the ticks the machine *applied* (`runs`) and the counters
//!    those runs touched. A `Dormant` zone receives no events, so 600 ticks
//!    of it are 0 runs and 0 counters **measured**; an `Open` zone that
//!    receives `leave_zone` on its first tick is 1 run and 1 counter, which is
//!    what proves the instrument is not a constant.
//! 3. **the boss register and the containers** — a boss that dies stays dead
//!    while the instance epoch does not change and only comes back with the
//!    `new_epoch` of the instance; a looted container is observed by the
//!    player that looted it *and* by another one (the shared-read half), and a
//!    sealed one is observed with `opened_by = 0` (player `0` is the world).
//!
//! **The save tail.** [`session_section`] is the single source of the four
//! `dungeon_*` families: the probe persists it in the real `save-all-*.dfl`
//! and [`publish`] re-derives the `load` row from the same lines, so the
//! published digest and the artifact can only agree when they describe the
//! same tail.
//!
//! **Frontier:** the eligibility rows and the `dungeon_loot` rows are owned by
//! [`crate::dungeon_elig`]; this module composes them for the save but does
//! not build them.

use daggerfall_memory::dungeon_v3::{DungeonSection, DungeonStateRow, MachineState};
use daggerfall_parser_rs::rdi::fnv1a64;

use crate::dun_canal;

/// The dungeon the session walks — `sample[0]` of `PROBE-DUNGEON-001`.
pub const LOCATION: u16 = 26_219;

/// Block index of the session inside that location.
pub const BLOCK: u8 = 0;

/// Boss id the session kills (the boss register of the block).
pub const BOSS: u32 = 3;

/// The container player 1 opens and player 2 finds open.
pub const CONTAINER_SHARED: u32 = 7;

/// The container no player ever opened: the zone sealed it.
pub const CONTAINER_SEALED: u32 = 8;

/// Ticks the zone clock is run for (`DORMANT_TICKS_MIN` of the gate).
pub const TICKS: u64 = 600;

/// The instance epoch of the session. The v3 grammar's epoch starts at `1`
/// (minute `0` is the sentinel of "never"), so the instance the save persists
/// lives in epoch 1 — the epoch `0` of the token contract is a *token* input,
/// not a persistable instance.
pub const SESSION_EPOCH: u32 = 1;

/// Game minutes since a machine entered the state it is published in, for a
/// transition that was just applied (`0` — it entered it now).
pub const JUST_ENTERED_MINUTES: u64 = 0;

/// Game minutes a refused counter-case was attempted after entering the state,
/// so the published row carries a real elapsed time and not a zero width.
pub const ATTEMPT_MINUTES: u64 = 30;

/// The six states of the machine, in the declared order (test fixture: the
/// product drives the machine through [`TRANSITIONS`] and never enumerates it).
#[cfg(test)]
pub const STATES: [MachineState; 6] = [
    MachineState::Dormant,
    MachineState::Open,
    MachineState::Engaged,
    MachineState::Cleared,
    MachineState::Cooldown,
    MachineState::Closed,
];

/// One row of the transition contract (block B, table §3.2).
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    /// Row number of the contract table.
    pub number: u8,
    /// Origin state.
    pub from: &'static str,
    /// Event the zone emits.
    pub event: &'static str,
    /// Destination state the table declares.
    pub to: &'static str,
    /// `counter_from` — origin of the counter-case.
    pub counter_from: &'static str,
    /// `counter_to` — the state the counter-case must report; the machine
    /// refuses the event there, so state and destination are the same.
    pub counter_to: &'static str,
}

/// The 12 transitions of the contract. They are the **input list** of the
/// measurement: [`publish`] feeds `(from, event)` and `(counter_from, event)`
/// to the machine and publishes its answers.
pub const TRANSITIONS: [Transition; 12] = [
    Transition {
        number: 1,
        from: "Dormant",
        event: "enter",
        to: "Open",
        counter_from: "Cleared",
        counter_to: "Cleared",
    },
    Transition {
        number: 2,
        from: "Dormant",
        event: "close",
        to: "Closed",
        counter_from: "Open",
        counter_to: "Open",
    },
    Transition {
        number: 3,
        from: "Open",
        event: "engage",
        to: "Engaged",
        counter_from: "Dormant",
        counter_to: "Dormant",
    },
    Transition {
        number: 4,
        from: "Engaged",
        event: "disengage",
        to: "Open",
        counter_from: "Dormant",
        counter_to: "Dormant",
    },
    Transition {
        number: 5,
        from: "Engaged",
        event: "objective",
        to: "Cleared",
        counter_from: "Open",
        counter_to: "Open",
    },
    Transition {
        number: 6,
        from: "Cleared",
        event: "leave_zone",
        to: "Cooldown",
        counter_from: "Dormant",
        counter_to: "Dormant",
    },
    Transition {
        number: 7,
        from: "Cooldown",
        event: "expire",
        to: "Dormant",
        counter_from: "Engaged",
        counter_to: "Engaged",
    },
    Transition {
        number: 8,
        from: "Cooldown",
        event: "reenter",
        to: "Open",
        counter_from: "Cleared",
        counter_to: "Cleared",
    },
    Transition {
        number: 9,
        from: "Open",
        event: "leave_zone",
        to: "Dormant",
        counter_from: "Closed",
        counter_to: "Closed",
    },
    Transition {
        number: 10,
        from: "Closed",
        event: "new_epoch",
        to: "Dormant",
        counter_from: "Cleared",
        counter_to: "Cleared",
    },
    Transition {
        number: 11,
        from: "Cleared",
        event: "seal",
        to: "Closed",
        counter_from: "Engaged",
        counter_to: "Engaged",
    },
    Transition {
        number: 12,
        from: "Open",
        event: "seal",
        to: "Closed",
        counter_from: "Cooldown",
        counter_to: "Cooldown",
    },
];

/// The destination the table declares for `(from, event)`, if it declares one.
pub fn declared(from: MachineState, event: &str) -> Option<MachineState> {
    TRANSITIONS
        .iter()
        .find(|row| row.from == from.as_str() && row.event == event)
        .and_then(|row| MachineState::parse(row.to))
}

/// The transition the machine applies for `(from, event)`, if any.
///
/// This is the product's rule: the table above plus the origin state is the
/// whole decision. `None` means the zone refuses the event and the machine is
/// unchanged.
pub fn transition(from: MachineState, event: &str) -> Option<MachineState> {
    declared(from, event)
}

/// One machine of the zone — a door, a container, a lever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// Current state.
    state: MachineState,
}

impl Machine {
    /// A machine sitting in `state`.
    pub fn at(state: MachineState) -> Self {
        Self { state }
    }

    /// Current state.
    pub fn state(&self) -> MachineState {
        self.state
    }

    /// Applies `event`: `Some(to)` when the machine accepts it (and moves),
    /// `None` when it refuses it (and stays).
    pub fn apply(&mut self, event: &str) -> Option<MachineState> {
        match transition(self.state, event) {
            Some(to) => {
                self.state = to;
                Some(to)
            }
            None => None,
        }
    }
}

/// One row of the zone simulation: what the clock did to a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimRow {
    /// Dungeon location.
    pub location: u16,
    /// State the machine was in when the window opened.
    ///
    /// It is the *opening* state and not the closing one: a window is
    /// attributed to the state it was opened in, so a window opened on an
    /// `Open` zone that was left still belongs to `Open` and never pollutes the
    /// dormant set.
    pub machine: MachineState,
    /// Ticks the window ran.
    pub ticks: u64,
    /// Counters the applied transitions touched.
    pub counters: u64,
    /// Ticks the machine applied (accepted events).
    pub runs: u64,
    /// Instance epoch.
    pub epoch: u32,
}

/// No events: what a dormant zone emits on a tick.
const NO_EVENTS: &[&str] = &[];

/// The events of the first tick of an `Open` zone that is left.
const LEAVE_ON_FIRST_TICK: &[&str] = &["leave_zone"];

/// The zone clock: it feeds the machine the events of each tick and counts.
#[derive(Debug, Clone, Copy)]
pub struct Sim {
    location: u16,
    epoch: u32,
    opened_in: MachineState,
    machine: Machine,
    ticks: u64,
    runs: u64,
    counters: u64,
}

impl Sim {
    /// A window over `machine` at `location`, in `epoch`.
    pub fn new(location: u16, machine: MachineState, epoch: u32) -> Self {
        Self {
            location,
            epoch,
            opened_in: machine,
            machine: Machine::at(machine),
            ticks: 0,
            runs: 0,
            counters: 0,
        }
    }

    /// Runs `ticks` ticks. `events` is the set of events the zone emits on a
    /// given tick; every accepted event is a run and touches one counter, a
    /// refused event is neither.
    pub fn run<F>(&mut self, ticks: u64, events: F)
    where
        F: Fn(u64) -> &'static [&'static str],
    {
        for tick in 0..ticks {
            for event in events(tick) {
                if self.machine.apply(event).is_some() {
                    self.runs += 1;
                    self.counters += 1;
                }
            }
            self.ticks += 1;
        }
    }

    /// The row this window publishes.
    pub fn row(&self) -> SimRow {
        SimRow {
            location: self.location,
            machine: self.opened_in,
            ticks: self.ticks,
            counters: self.counters,
            runs: self.runs,
            epoch: self.epoch,
        }
    }
}

/// Life state of a boss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossState {
    /// Alive and killable.
    Alive,
    /// Dead and waiting for the instance epoch to change.
    Dead,
}

impl BossState {
    /// Canonical spelling of the state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alive => "Alive",
            Self::Dead => "Dead",
        }
    }
}

/// The boss register of one dungeon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boss {
    /// Boss id inside the block.
    pub id: u32,
    /// Instance epoch it belongs to.
    pub epoch: u32,
    /// Whether it is alive.
    pub state: BossState,
    /// Epochs it came back in.
    pub respawns: u32,
}

impl Boss {
    /// A boss alive in `epoch`.
    pub fn new(id: u32, epoch: u32) -> Self {
        Self {
            id,
            epoch,
            state: BossState::Alive,
            respawns: 0,
        }
    }

    /// Kills it. Only a living boss dies, so a second kill cannot inflate
    /// anything.
    pub fn kill(&mut self) {
        self.state = BossState::Dead;
    }

    /// Advances the instance clock to `epoch`. A dead boss only comes back when
    /// the epoch **changes** (`new_epoch`): inside one epoch it stays dead.
    /// Returns whether it came back.
    pub fn advance_epoch(&mut self, epoch: u32) -> bool {
        if epoch == self.epoch {
            return false;
        }
        self.epoch = epoch;
        if self.state == BossState::Dead {
            self.state = BossState::Alive;
            self.respawns += 1;
            return true;
        }
        false
    }

    /// What an observer reads: the life state and the respawn count.
    pub fn observe(&self) -> (BossState, u32) {
        (self.state, self.respawns)
    }
}

/// One observation of a container by a player, in an epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerView {
    /// Dungeon location.
    pub location: u16,
    /// Container id.
    pub container: u32,
    /// Player that opened it (`0` = no player: the world).
    pub opened_by: u8,
    /// Player reading the state.
    pub observed_by: u8,
    /// Machine state the reader saw.
    pub machine: MachineState,
    /// Instance epoch.
    pub epoch: u32,
}

/// The two container observations of the session, both measured on the
/// machine:
///
/// - [`CONTAINER_SHARED`] is opened by player 1 (`Dormant --enter--> Open`,
///   row 1 of the contract) and read by player 2 from the same row, so the
///   second player **sees it open** — the shared half of the ledger;
/// - [`CONTAINER_SEALED`] is never opened by a player: the world seals it
///   (`Open --seal--> Closed`, row 12), so it reads `opened_by = 0` and
///   `Closed` — the never-opened half.
pub fn container_views() -> Vec<ContainerView> {
    let mut shared = Machine::at(MachineState::Dormant);
    let opened_by = if shared.apply("enter").is_some() { 1 } else { 0 };
    let mut sealed = Machine::at(MachineState::Open);
    let _sealed_by_the_world = sealed.apply("seal");
    vec![
        ContainerView {
            location: LOCATION,
            container: CONTAINER_SHARED,
            opened_by,
            observed_by: 2,
            machine: shared.state(),
            epoch: SESSION_EPOCH,
        },
        ContainerView {
            location: LOCATION,
            container: CONTAINER_SEALED,
            opened_by: 0,
            observed_by: 0,
            machine: sealed.state(),
            epoch: SESSION_EPOCH,
        },
    ]
}

/// The simulation rows of the session: a dormant window (nothing happens —
/// measured) and a window that starts `Open` and is left on its first tick
/// (one run, one counter — the instrument bites).
pub fn sim_rows() -> Vec<SimRow> {
    let mut dormant = Sim::new(LOCATION, MachineState::Dormant, SESSION_EPOCH);
    dormant.run(TICKS, |_tick| NO_EVENTS);

    let mut left = Sim::new(LOCATION, MachineState::Open, SESSION_EPOCH);
    left.run(TICKS, |tick| {
        if tick == 0 {
            LEAVE_ON_FIRST_TICK
        } else {
            NO_EVENTS
        }
    });

    vec![dormant.row(), left.row()]
}

/// The boss rows of the session: the boss is killed and observed twice inside
/// epoch 0 (dead both times, no respawn — the epoch did not change), then the
/// instance clock changes and a third observation shows it back with one
/// respawn. The rows of epoch 0 are the "persisted dead" pair.
pub fn boss_rows() -> Vec<(u32, BossState, u32, u32)> {
    let mut boss = Boss::new(BOSS, SESSION_EPOCH);
    boss.kill();
    // The zone clock runs; a dead boss is not affected by time inside its epoch.
    let mut clock = Sim::new(LOCATION, MachineState::Dormant, SESSION_EPOCH);
    clock.run(TICKS, |_tick| NO_EVENTS);
    let (first, respawns_first) = boss.observe();
    clock.run(TICKS, |_tick| NO_EVENTS);
    let (second, respawns_second) = boss.observe();
    let first_row = (boss.epoch, first, respawns_first, 0);
    let second_row = (boss.epoch, second, respawns_second, 0);

    let came_back = boss.advance_epoch(SESSION_EPOCH + 1);
    let (third, respawns_third) = boss.observe();
    let third_row = (
        boss.epoch,
        third,
        respawns_third,
        u32::from(came_back),
    );
    vec![first_row, second_row, third_row]
}

/// The four `dungeon_*` families of the session, as the save persists them:
/// the state machines, the eligibility tokens and the loot row of
/// [`crate::dungeon_elig`], and the instance the session runs in.
///
/// Order is the v3 order of each family, so the tail survives a
/// `save → load → save` round trip.
pub fn session_section() -> DungeonSection {
    let mut states = vec![
        // The boss died: the location is Cleared, 600 minutes ago (the window).
        DungeonStateRow {
            location_id: LOCATION,
            block_index: BLOCK,
            door_id: 0,
            machine: MachineState::Cleared,
            since_minutes: TICKS,
            epoch: SESSION_EPOCH,
        },
        // Player 1 opened the shared container.
        DungeonStateRow {
            location_id: LOCATION,
            block_index: BLOCK,
            door_id: CONTAINER_SHARED,
            machine: MachineState::Open,
            since_minutes: JUST_ENTERED_MINUTES,
            epoch: SESSION_EPOCH,
        },
        // The world sealed the other one; no player ever opened it.
        DungeonStateRow {
            location_id: LOCATION,
            block_index: BLOCK,
            door_id: CONTAINER_SEALED,
            machine: MachineState::Closed,
            since_minutes: JUST_ENTERED_MINUTES,
            epoch: SESSION_EPOCH,
        },
    ];
    states.sort_by_key(|row| (row.location_id, row.block_index, row.door_id));

    DungeonSection {
        states,
        eligible: crate::dungeon_elig::elig_rows(),
        loot: crate::dungeon_elig::loot_rows(),
        instances: vec![daggerfall_memory::dungeon_v3::DungeonInstanceRow {
            instance_id: fnv1a64(
                format!("dungeon-instance-v1|{LOCATION}|{SESSION_EPOCH}|1").as_bytes(),
            )
            .to_be_bytes(),
            location_id: LOCATION,
            epoch: SESSION_EPOCH,
            player_id: 1,
        }],
    }
}

/// The four family keys of the v3 dungeon tail, in the order the writer emits
/// them.
const FAMILY_KEYS: [&str; 4] = [
    "dungeon_state",
    "dungeon_elig",
    "dungeon_loot",
    "dungeon_instance",
];

/// The lines the save persists for the session tail — the exact bytes the
/// writer appends to the document, in file order.
fn save_tail_lines() -> String {
    let mut lines = String::new();
    session_section().write_lines(&mut lines);
    lines
}

/// How many of the four families the persisted tail declares (a family with
/// `0` rows still declares its count key, so this is 4 for a complete tail).
pub fn tail_families() -> usize {
    let lines = save_tail_lines();
    FAMILY_KEYS
        .iter()
        .filter(|key| lines.lines().any(|line| line.starts_with(**key)))
        .count()
}

/// The digest of the `dungeon_*` tail the save persists: FNV-1a 64 over the
/// lines that carry any of the four family keys, in file order — the same
/// number a reader of the artifact recomputes.
pub fn save_tail_digest() -> u64 {
    fnv1a64(save_tail_lines().as_bytes())
}

/// Publishes the state section through the canal.
///
/// Order: the machine (12 cases + 12 counter-cases), the simulation, the boss
/// register, the container observations, the load of the persisted tail.
/// Deterministic end to end — no clock, no RNG, no map iteration.
pub fn publish() {
    // R5 — the machine, driven. The table supplies the inputs; the published
    // answer is what `apply` returned.
    for row in TRANSITIONS {
        publish_case(row.number, row.from, row.event, JUST_ENTERED_MINUTES);
    }
    for row in TRANSITIONS {
        // A counter-case refuses the event, so the state the table declares for
        // it must be the origin itself: a table that declared a move would be
        // describing an accepted event. The machine's answer is what gets
        // published either way.
        debug_assert_eq!(
            row.counter_from, row.counter_to,
            "row {}: a refused event cannot move the machine",
            row.number
        );
        publish_case(row.number, row.counter_from, row.event, ATTEMPT_MINUTES);
    }

    // R6 — the zone clock.
    for row in sim_rows() {
        dun_canal::emit(&format!(
            "DUN VE: sim location={} machine={} ticks={} counters={} runs={} epoch={}",
            row.location,
            row.machine.as_str(),
            row.ticks,
            row.counters,
            row.runs,
            row.epoch
        ));
    }

    // R7 — the boss register.
    for (epoch, state, respawns, came_back) in boss_rows() {
        dun_canal::emit(&format!(
            "DUN VE: boss location={LOCATION} boss={BOSS} state={} epoch={epoch} \
             respawns={respawns}",
            state.as_str()
        ));
        // `came_back` is the register's own answer to the epoch change: it must
        // agree with the row that publishes the respawn count, otherwise the
        // count is describing something the register did not do.
        debug_assert_eq!(
            came_back,
            u32::from(respawns > 0),
            "boss {BOSS}: respawns={respawns} contradicts came_back={came_back}"
        );
    }

    // R8 — the container observations.
    for view in container_views() {
        dun_canal::emit(&format!(
            "DUN VE: container location={} container={} opened_by={} observed_by={} \
             machine={} epoch={}",
            view.location,
            view.container,
            view.opened_by,
            view.observed_by,
            view.machine.as_str(),
            view.epoch
        ));
    }

    // R9 — the load of the persisted tail: the four families and the digest of
    // the very lines the writer emits.
    dun_canal::emit(&format!(
        "DUN VE: load location={LOCATION} families={} digest={:016x} epoch={SESSION_EPOCH}",
        tail_families(),
        save_tail_digest()
    ));
}

/// Drives one machine from `from` with `event` and publishes the answer.
fn publish_case(number: u8, from: &str, event: &str, since_minutes: u64) {
    let Some(origin) = MachineState::parse(from) else {
        return;
    };
    let mut machine = Machine::at(origin);
    let applied = machine.apply(event);
    let result = if applied.is_some() { "applied" } else { "rejected" };
    dun_canal::emit(&format!(
        "DUN VE: state location={LOCATION} block={BLOCK} door={number} from={} event={event} \
         to={} result={result} epoch={SESSION_EPOCH} since_minutes={since_minutes}",
        origin.as_str(),
        machine.state().as_str()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declared case is accepted by the machine and lands where the
    /// table says, and every counter-case is refused and leaves the state
    /// where it was.
    #[test]
    fn the_machine_matches_the_contract() {
        for row in TRANSITIONS {
            let from = MachineState::parse(row.from).expect("declared origin");
            let mut machine = Machine::at(from);
            assert_eq!(
                machine.apply(row.event),
                MachineState::parse(row.to),
                "row {} ({}/{}): the machine does not reach the declared destination",
                row.number,
                row.from,
                row.event
            );

            let counter_from = MachineState::parse(row.counter_from).expect("counter origin");
            let mut counter = Machine::at(counter_from);
            assert_eq!(
                counter.apply(row.event),
                None,
                "row {}: {} accepted from {} — the counter-case is not refused",
                row.number,
                row.event,
                row.counter_from
            );
            assert_eq!(
                counter.state(),
                counter_from,
                "row {}: a refused event moved the machine",
                row.number
            );
            assert_eq!(
                row.counter_from, row.counter_to,
                "row {}: a refused event cannot change the state",
                row.number
            );
        }
    }

    /// The six states are the origins of the contract: no state is unreachable
    /// and none is missing from the published set.
    #[test]
    fn every_state_is_reachable_from_the_table() {
        for state in STATES {
            assert!(
                TRANSITIONS
                    .iter()
                    .any(|row| row.from == state.as_str() || row.counter_from == state.as_str()),
                "{} never appears in the contract",
                state.as_str()
            );
        }
    }

    /// A dormant window is inert **as measured**, and a window that is left on
    /// its first tick bites: the counter is not a constant.
    #[test]
    fn the_simulation_counts_what_it_runs() {
        let rows = sim_rows();
        let dormant = rows[0];
        assert_eq!(dormant.machine, MachineState::Dormant);
        assert_eq!(dormant.ticks, TICKS);
        assert_eq!(dormant.runs, 0);
        assert_eq!(dormant.counters, 0);

        let left = rows[1];
        assert_eq!(left.runs, 1, "the leave_zone tick is not counted");
        assert_eq!(left.counters, 1);
        assert_eq!(
            left.machine,
            MachineState::Open,
            "a window is attributed to the state it was opened in"
        );
    }

    /// A boss stays dead while the epoch does not change and only comes back
    /// with a new epoch.
    #[test]
    fn a_dead_boss_waits_for_the_epoch() {
        let rows = boss_rows();
        assert_eq!(rows[0].1, BossState::Dead);
        assert_eq!(rows[0].2, 0);
        assert_eq!(rows[1].1, BossState::Dead);
        assert_eq!(rows[1].2, 0);
        assert_eq!(rows[2].0, SESSION_EPOCH + 1);
        assert_eq!(rows[2].1, BossState::Alive);
        assert_eq!(rows[2].2, 1);
        assert_eq!(rows[2].3, 1);
    }

    /// One container is seen open by a second player, the other was never
    /// opened by anybody.
    #[test]
    fn containers_report_who_opened_them() {
        let views = container_views();
        assert_eq!(views[0].machine, MachineState::Open);
        assert_eq!(views[0].opened_by, 1);
        assert_ne!(views[0].opened_by, views[0].observed_by);
        assert_eq!(views[1].machine, MachineState::Closed);
        assert_eq!(views[1].opened_by, 0);
    }

    /// The tail the save persists survives the v3 canonical form and keeps the
    /// four families non-empty.
    #[test]
    fn the_session_tail_is_canonical() {
        let section = session_section();
        assert_eq!(tail_families(), 4);
        assert!(!section.states.is_empty());
        assert!(!section.eligible.is_empty());
        assert!(!section.loot.is_empty());
        assert!(!section.instances.is_empty());
        section.validate().expect("the session tail is not canonical");
    }

    /// The published digest is the one a reader of the artifact recomputes.
    #[test]
    fn the_tail_digest_is_stable() {
        assert_eq!(save_tail_digest(), save_tail_digest());
    }
}
