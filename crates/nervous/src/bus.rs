//! Spine event bus (contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §6).
//!
//! `publish` enqueues, `drain_tick` hands the queue out **in publication
//! order** and empties it. The capacity is a contract value, not a tuning
//! knob: at [`EventBus::DEFAULT_CAP`] the 4 097th `publish` of a tick returns
//! [`BusError::Full`] — the bus **never** drops an event silently, because a
//! dropped event is an untraceable divergence between two runs of the same
//! seed (the determinism falsifier of §9).
//!
//! The bus mutates **nothing** but its own queue: it holds no session, no
//! scene and no organ state (comunicación directa, §13 — organs publish and
//! subscribe, they never call each other).

use std::fmt;

use crate::events::GameEvent;

/// Events a tick can hold before the bus refuses more (contract §6: 4 096).
pub const DEFAULT_EVENT_CAP: usize = 4096;

/// Why an event could not be published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    /// The tick queue is at capacity; the event was **not** queued.
    Full,
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => write!(
                f,
                "event bus full (cap {DEFAULT_EVENT_CAP} events/tick): event refused, 0 silent drops"
            ),
        }
    }
}

impl std::error::Error for BusError {}

/// FIFO event queue for one tick (contract §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBus {
    queue: Vec<GameEvent>,
    cap: usize,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Contract capacity (4 096 events/tick).
    pub const DEFAULT_CAP: usize = DEFAULT_EVENT_CAP;

    /// Bus with the contract capacity.
    pub fn new() -> Self {
        Self::with_cap(Self::DEFAULT_CAP)
    }

    /// Bus with an explicit capacity. Only tests and probes shrink it; the
    /// game always uses [`Self::new`].
    pub fn with_cap(cap: usize) -> Self {
        Self {
            queue: Vec::new(),
            cap,
        }
    }

    /// Capacity of this bus.
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Queued events.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// No event queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Enqueues `ev` at the tail. Returns [`BusError::Full`] (and queues
    /// nothing) when the tick is at capacity.
    pub fn publish(&mut self, ev: GameEvent) -> Result<(), BusError> {
        if self.queue.len() >= self.cap {
            return Err(BusError::Full);
        }
        self.queue.push(ev);
        Ok(())
    }

    /// Hands out every queued event in **publication order** and empties the
    /// queue.
    ///
    /// Allocation shape: one buffer sized exactly to the drained tick, while
    /// this bus keeps its own capacity — no growth/realloc per tick, no
    /// per-event copy of the queue.
    pub fn drain_tick(&mut self) -> Vec<GameEvent> {
        let mut out = Vec::with_capacity(self.queue.len());
        out.append(&mut self.queue);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{HourBand, LocationRef, WindowId};

    #[test]
    fn contract_capacity_is_4096() {
        assert_eq!(EventBus::new().cap(), 4096);
        assert_eq!(EventBus::DEFAULT_CAP, 4096);
    }

    #[test]
    fn publishes_in_order_and_drains() {
        let mut bus = EventBus::new();
        let door = GameEvent::DoorActivated {
            location: LocationRef::new(19, 0, 3),
            block: 7,
            door_id: 2,
        };
        let band = GameEvent::DayNightChanged(HourBand::Night);
        let ui = GameEvent::UiRequested(WindowId::QUESTS);
        for ev in [door, band, ui] {
            assert!(bus.publish(ev).is_ok());
        }
        assert_eq!(bus.len(), 3);
        assert_eq!(bus.drain_tick(), vec![door, band, ui]);
        assert!(bus.is_empty());
    }

    #[test]
    fn overflow_is_an_error_not_a_drop() {
        let mut bus = EventBus::with_cap(2);
        let ev = GameEvent::WeatherChanged(crate::events::WeatherType::Rain);
        assert!(bus.publish(ev).is_ok());
        assert!(bus.publish(ev).is_ok());
        assert_eq!(bus.publish(ev), Err(BusError::Full));
        assert_eq!(bus.len(), 2, "the refused event must not be queued");
        assert_eq!(bus.drain_tick().len(), 2);
    }

    #[test]
    fn drain_keeps_the_bus_capacity() {
        let mut bus = EventBus::with_cap(4);
        for _ in 0..4 {
            bus.publish(GameEvent::UiRequested(WindowId::HUD))
                .expect("under cap");
        }
        assert_eq!(bus.drain_tick().len(), 4);
        assert_eq!(bus.cap(), 4);
        assert!(bus.is_empty());
    }
}
