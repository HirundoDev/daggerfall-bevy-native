//! `scheduler` — deterministic phase scheduler (PROBE-SCHEDULE-001, M9).
//!
//! ❤️ Heart organ: cadence. The scheduler drains the events scheduled for a
//! tick in a **total order** `(priority(phase), seq)` — never a hash order —
//! and emits the cadence event `TimeAdvanced` first whenever time actually
//! advances.
//!
//! Design rules:
//! - priorities come from a fixed `[u8; 3]` table indexed by phase
//!   (`0 = Input`, `1 = Simulation`, `2 = Presentation`; default `0, 1, 2`),
//!   so a mutated table changes the emitted order observably;
//! - `seq` is the insertion counter: within the same priority the insertion
//!   order decides, which makes the order deterministic and falsifiable
//!   (inserting two events in the opposite order yields the opposite log);
//! - `tick` advances a real [`GameClock`] through `raise_seconds` (integer
//!   carry, no `f32`).

use crate::clock::GameClock;

/// Execution phase of a scheduled event (the three stages of a game tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SchedulePhase {
    /// Player/world input collection.
    Input,
    /// World simulation.
    Simulation,
    /// Presentation (camera, UI, audio).
    Presentation,
}

impl SchedulePhase {
    /// Slot of this phase in the priority table.
    pub const fn priority_slot(self) -> usize {
        match self {
            Self::Input => 0,
            Self::Simulation => 1,
            Self::Presentation => 2,
        }
    }

    /// Stable label used by evidence output.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Simulation => "simulation",
            Self::Presentation => "presentation",
        }
    }
}

/// One emitted event: tick, phase, insertion sequence and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimEvent {
    /// Tick (1-based) in which the event was emitted.
    pub tick: u64,
    /// Phase of the event.
    pub phase: SchedulePhase,
    /// Insertion sequence (0-based) of the scheduled event; `0` for the
    /// cadence event `TimeAdvanced`.
    pub seq: u64,
    /// Event name (static: the scheduler carries no dynamic payload).
    pub name: &'static str,
}

/// Name of the cadence event emitted by every tick that advances time.
pub const TIME_ADVANCED: &str = "TimeAdvanced";

#[derive(Debug, Clone)]
struct Pending {
    seq: u64,
    phase: SchedulePhase,
    name: &'static str,
}

/// Deterministic phase scheduler.
#[derive(Debug, Clone)]
pub struct Scheduler {
    priorities: [u8; 3],
    pending: Vec<Pending>,
    next_seq: u64,
    tick: u64,
}

impl Scheduler {
    /// Scheduler with the default priorities (`Input=0`, `Simulation=1`,
    /// `Presentation=2`).
    pub fn new() -> Self {
        Self::with_priorities([0, 1, 2])
    }

    /// Scheduler with explicit priorities: index `0 = Input`,
    /// `1 = Simulation`, `2 = Presentation` (lower drains first).
    pub fn with_priorities(priorities: [u8; 3]) -> Self {
        Self {
            priorities,
            pending: Vec::new(),
            next_seq: 0,
            tick: 0,
        }
    }

    /// Schedules `name` in `phase` and returns its insertion sequence.
    pub fn schedule(&mut self, phase: SchedulePhase, name: &'static str) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push(Pending { seq, phase, name });
        seq
    }

    /// Pending events not yet drained.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Number of ticks executed so far.
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Runs one tick: advances `clock` by `seconds` (when `> 0`) and drains
    /// every pending event in `(priority(phase), seq)` order.
    ///
    /// `TimeAdvanced` (phase [`SchedulePhase::Simulation`]) is always emitted
    /// FIRST when `seconds > 0`; the remaining events follow sorted by the
    /// priority table and then by insertion sequence.
    pub fn tick(&mut self, clock: &mut GameClock, seconds: u64) -> Vec<SimEvent> {
        self.tick += 1;
        let tick = self.tick;
        let mut events = Vec::with_capacity(self.pending.len() + 1);

        if seconds > 0 {
            clock.raise_seconds(seconds);
            events.push(SimEvent {
                tick,
                phase: SchedulePhase::Simulation,
                seq: 0,
                name: TIME_ADVANCED,
            });
        }

        let mut drained = std::mem::take(&mut self.pending);
        let priorities = self.priorities;
        drained.sort_by_key(|p| (priorities[p.phase.priority_slot()], p.seq));
        events.extend(drained.into_iter().map(|p| SimEvent {
            tick,
            phase: p.phase,
            seq: p.seq,
            name: p.name,
        }));
        events
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed initial state + 2 ticks → log of `(phase, name)` per tick.
///
/// Initial state (identical on every call): clock
/// [`GameClock::classic_default`], events `Input/"MoveIntent"`,
/// `Simulation/"CollisionResolve"`, `Simulation/"DamageApply"`,
/// `Presentation/"CameraUpdate"`, `Input/"UseIntent"`; each tick raises the
/// clock by 600 seconds.
pub fn two_tick_log(priorities: [u8; 3]) -> Vec<Vec<(SchedulePhase, &'static str)>> {
    let mut clock = GameClock::classic_default();
    let mut scheduler = Scheduler::with_priorities(priorities);
    scheduler.schedule(SchedulePhase::Input, "MoveIntent");
    scheduler.schedule(SchedulePhase::Simulation, "CollisionResolve");
    scheduler.schedule(SchedulePhase::Simulation, "DamageApply");
    scheduler.schedule(SchedulePhase::Presentation, "CameraUpdate");
    scheduler.schedule(SchedulePhase::Input, "UseIntent");

    let mut log = Vec::with_capacity(2);
    for _ in 0..2 {
        log.push(
            scheduler
                .tick(&mut clock, 600)
                .into_iter()
                .map(|event| (event.phase, event.name))
                .collect(),
        );
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_two_tick_log_is_deterministic() {
        assert_eq!(two_tick_log([0, 1, 2]), two_tick_log([0, 1, 2]));
    }

    #[test]
    fn default_order_is_cadence_then_priority_then_seq() {
        let log = two_tick_log([0, 1, 2]);
        assert_eq!(
            log[0],
            vec![
                (SchedulePhase::Simulation, TIME_ADVANCED),
                (SchedulePhase::Input, "MoveIntent"),
                (SchedulePhase::Input, "UseIntent"),
                (SchedulePhase::Simulation, "CollisionResolve"),
                (SchedulePhase::Simulation, "DamageApply"),
                (SchedulePhase::Presentation, "CameraUpdate"),
            ]
        );
        // Second tick: no pending events, only the cadence event.
        assert_eq!(log[1], vec![(SchedulePhase::Simulation, TIME_ADVANCED)]);
    }

    #[test]
    fn mutated_priority_changes_the_order() {
        assert_ne!(two_tick_log([0, 1, 2]), two_tick_log([1, 0, 2]));
    }

    #[test]
    fn insertion_order_decides_within_the_same_priority() {
        let mut first = Scheduler::new();
        first.schedule(SchedulePhase::Simulation, "A");
        first.schedule(SchedulePhase::Simulation, "B");
        let mut second = Scheduler::new();
        second.schedule(SchedulePhase::Simulation, "B");
        second.schedule(SchedulePhase::Simulation, "A");

        let mut clock = GameClock::classic_default();
        let first_names: Vec<&'static str> = first
            .tick(&mut clock, 0)
            .into_iter()
            .map(|e| e.name)
            .collect();
        let second_names: Vec<&'static str> = second
            .tick(&mut clock, 0)
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(first_names, vec!["A", "B"]);
        assert_eq!(second_names, vec!["B", "A"]);
        assert_ne!(first_names, second_names);
    }

    #[test]
    fn tick_advances_the_clock_and_emits_cadence_first() {
        let mut clock = GameClock::classic_default();
        let mut scheduler = Scheduler::new();
        scheduler.schedule(SchedulePhase::Input, "MoveIntent");
        let events = scheduler.tick(&mut clock, 600);
        assert_eq!(events[0].name, TIME_ADVANCED);
        assert_eq!(events[0].tick, 1);
        assert_eq!(clock.hour, 12);
        assert_eq!(clock.minute, 10);
        assert_eq!(scheduler.pending_len(), 0);
        // A zero-second tick does not emit the cadence event nor advance.
        let before = clock;
        assert!(scheduler.tick(&mut clock, 0).is_empty());
        assert_eq!(clock, before);
    }
}
