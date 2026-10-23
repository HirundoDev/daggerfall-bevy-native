//! M29 · Quest guardrails (SPEC-M29-QUESTS; REQ-L99-062).
//!
//! The `behavior` organ runs the quest machine; this module states the
//! invariants every observable quest value must satisfy, so a wrong clock,
//! a silent NEGATIVE opcode or a duplicated journal step becomes a typed
//! violation instead of a plausible-looking journal.
//!
//! The organ stays dependency-free: the source constants are restated here
//! (`Quest.cs:373,382-386`, `Clock.cs:276-300`, `Quest.cs:52,388-398`,
//! `Actions/JournalNote.cs:50-52`, `Task.cs:211-212,263-276`), never imported
//! from `behavior`. The formulas are not duplicated; a guardrail that
//! recomputed them would pass by construction.

/// `ticksToEnd = 2` (`Quest.cs:373`).
pub const END_TICKS_TO_END: u32 = 2;
/// `QuestSuccessRep = 5` (`Quest.cs:36`).
pub const SUCCESS_REPUTATION: i16 = 5;
/// `QuestFailureRep = -2` (`Quest.cs:37`).
pub const FAILURE_REPUTATION: i16 = -2;
/// Message sentinel (`spec 2.5`).
pub const NO_MESSAGE_ID: u16 = 0xFFFF;
/// Journal notes start at this step id (the target encoding of
/// `Actions/JournalNote.cs:50-52`).
pub const NOTE_STEP_BASE: u32 = 0x8000_0000;

/// Typed invariant violation of the quest domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestViolation {
    /// A clock remaining value is negative (`Clock.cs:287,296`).
    NegativeClock {
        /// Observed seconds.
        remaining: i64,
    },
    /// `ticksToEnd` outside `{0,1,2}` (`Quest.cs:300-305,373`).
    TicksToEndOutOfRange {
        /// Observed ticks.
        ticks: u32,
    },
    /// Two journal entries share a step id (`Quest.cs:52,87-92`).
    DuplicateJournalStep {
        /// Repeated id.
        step_id: u32,
    },
    /// An absent message id was treated as success
    /// (`Actions/JournalNote.cs:50-52`; spec 6.3 C).
    MissingMessageTreatedAsSuccess {
        /// Observed id.
        message_id: u16,
    },
    /// A NEGATIVE opcode ran without a recorded failure (`spec 2.4`).
    UnsupportedOpcodeWithoutFailure {
        /// Type byte.
        type_id: u8,
    },
    /// Closing reputation outside `{0,+5,-2}` (`Quest.cs:382-386`).
    ClosingReputationOutOfRange {
        /// Faction id.
        faction_id: u16,
        /// Observed delta.
        delta: i16,
    },
    /// A `PersistUntil` task is still triggered while its target is
    /// triggered (`Task.cs:263-270`).
    PersistUntilTriggeredWithActiveTarget {
        /// Persist task.
        task: String,
        /// Target task.
        target: String,
    },
    /// A dropped task executed (`Task.cs:303-308`, `Quest.cs:315-318`).
    DroppedTaskExecuted {
        /// Dropped task.
        task: String,
    },
    /// A note or a missing message reached the finished journal
    /// (`Quest.cs:388-398`).
    InvalidFinishedEntry {
        /// Step id.
        step_id: u32,
        /// Message id.
        message_id: u16,
    },
    /// A display-only message (`say`, type `0x33`) was written to the journal:
    /// `AddFinishedQuest` receives only the `log` steps (spec 6.3 B,
    /// `Quest.cs:388-398`).
    DisplayMessageJournaled {
        /// Displayed message id.
        message_id: u16,
    },
    /// A task is triggered at birth only if its kind starts on its own
    /// (`Task.cs:181-185`: `Headless` sets `IsTriggered = true`).
    TaskBirthStateMismatch {
        /// The kind starts on its own.
        auto_start: bool,
        /// Observed `IsTriggered` at construction.
        is_triggered: bool,
    },
    /// A missing message returned before `QuestBreak`
    /// (`Quest.cs:780-782`); the break belongs to a queued popup
    /// (`:838-841`).
    MissingMessageBrokeTick {
        /// Missing message id.
        message_id: u16,
    },
}

/// A clock remaining value is never negative.
pub fn check_clock_remaining(remaining: i64) -> Result<i64, QuestViolation> {
    if remaining < 0 {
        Err(QuestViolation::NegativeClock { remaining })
    } else {
        Ok(remaining)
    }
}

/// `ticksToEnd` is `0` (running) or at most the close countdown.
pub fn check_ticks_to_end(ticks: u32) -> Result<u32, QuestViolation> {
    if ticks <= END_TICKS_TO_END {
        Ok(ticks)
    } else {
        Err(QuestViolation::TicksToEndOutOfRange { ticks })
    }
}

/// Journal step ids are unique.
pub fn check_journal_steps(steps: &[u32]) -> Result<(), QuestViolation> {
    for (index, step) in steps.iter().enumerate() {
        if steps[..index].contains(step) {
            return Err(QuestViolation::DuplicateJournalStep { step_id: *step });
        }
    }
    Ok(())
}

/// The sentinel `0xFFFF` and absent ids are never a success.
pub fn check_missing_message(
    message_id: u16,
    treated_as_success: bool,
) -> Result<(), QuestViolation> {
    if message_id == NO_MESSAGE_ID && treated_as_success {
        Err(QuestViolation::MissingMessageTreatedAsSuccess { message_id })
    } else {
        Ok(())
    }
}

/// A NEGATIVE type must have produced an explicit failure (never silence).
pub fn check_unsupported_opcode(type_id: u8, has_failure: bool) -> Result<(), QuestViolation> {
    if has_failure {
        Ok(())
    } else {
        Err(QuestViolation::UnsupportedOpcodeWithoutFailure { type_id })
    }
}

/// Closing reputation only takes the three measured shapes: no faction
/// (`0`), success (`+5`) or failure (`-2`), the latter two with a faction.
pub fn check_closing_reputation(faction_id: u16, delta: i16) -> Result<(), QuestViolation> {
    let valid = if faction_id == 0 {
        delta == 0
    } else {
        delta == SUCCESS_REPUTATION || delta == FAILURE_REPUTATION
    };
    if valid {
        Ok(())
    } else {
        Err(QuestViolation::ClosingReputationOutOfRange { faction_id, delta })
    }
}

/// A `PersistUntil` task cannot stay triggered once its target is triggered.
pub fn check_persist_until(
    task: &str,
    target: &str,
    task_triggered: bool,
    target_triggered: bool,
) -> Result<(), QuestViolation> {
    if task_triggered && target_triggered {
        Err(QuestViolation::PersistUntilTriggeredWithActiveTarget {
            task: task.to_string(),
            target: target.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Dropped tasks never execute (`Quest.cs:315-318`).
pub fn check_task_executed(task: &str, dropped: bool, executed: bool) -> Result<(), QuestViolation> {
    if dropped && executed {
        Err(QuestViolation::DroppedTaskExecuted {
            task: task.to_string(),
        })
    } else {
        Ok(())
    }
}

/// The finished journal only carries log steps (`step_id < NOTE_STEP_BASE`)
/// whose message exists (`Quest.cs:388-398`).
pub fn check_finished_journal(entries: &[(u32, u16)]) -> Result<(), QuestViolation> {
    for (step_id, message_id) in entries {
        if *step_id >= NOTE_STEP_BASE || *message_id == NO_MESSAGE_ID {
            return Err(QuestViolation::InvalidFinishedEntry {
                step_id: *step_id,
                message_id: *message_id,
            });
        }
    }
    Ok(())
}

/// A displayed message never appears among the journal message ids: only
/// `log` entries are journaled (`Actions/Say.cs:58-62`, `Quest.cs:388-398`).
pub fn check_display_message_not_journaled(
    message_id: u16,
    journal_message_ids: &[u16],
) -> Result<(), QuestViolation> {
    if journal_message_ids.contains(&message_id) {
        Err(QuestViolation::DisplayMessageJournaled { message_id })
    } else {
        Ok(())
    }
}

/// A task begins triggered exactly when its kind starts on its own
/// (`Task.cs:181-185`; the other four kinds wait for `set`/a trigger).
pub fn check_task_birth_state(
    auto_start: bool,
    is_triggered: bool,
) -> Result<(), QuestViolation> {
    if auto_start == is_triggered {
        Ok(())
    } else {
        Err(QuestViolation::TaskBirthStateMismatch {
            auto_start,
            is_triggered,
        })
    }
}

/// A missing message never breaks the quest tick: `GetMessage` returns
/// `null` and the caller returns before `QuestBreak` (`Quest.cs:780-782`).
pub fn check_missing_message_break(
    message_id: u16,
    message_exists: bool,
    quest_break: bool,
) -> Result<(), QuestViolation> {
    if !message_exists && quest_break {
        Err(QuestViolation::MissingMessageBrokeTick { message_id })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_and_close_bounds() {
        assert_eq!(check_clock_remaining(0), Ok(0));
        assert!(matches!(
            check_clock_remaining(-1),
            Err(QuestViolation::NegativeClock { .. })
        ));
        assert_eq!(check_ticks_to_end(2), Ok(2));
        assert!(matches!(
            check_ticks_to_end(3),
            Err(QuestViolation::TicksToEndOutOfRange { ticks: 3 })
        ));
    }

    #[test]
    fn journal_steps_are_unique() {
        assert_eq!(check_journal_steps(&[1, 2, 3]), Ok(()));
        assert_eq!(
            check_journal_steps(&[1, 2, 1]),
            Err(QuestViolation::DuplicateJournalStep { step_id: 1 })
        );
    }

    #[test]
    fn missing_messages_never_pass_as_success() {
        assert!(check_missing_message(NO_MESSAGE_ID, false).is_ok());
        assert!(check_missing_message(NO_MESSAGE_ID, true).is_err());
        assert!(check_missing_message(7, true).is_ok());
        assert!(check_finished_journal(&[(1, 7)]).is_ok());
        assert!(check_finished_journal(&[(NOTE_STEP_BASE, 7)]).is_err());
        assert!(check_finished_journal(&[(1, NO_MESSAGE_ID)]).is_err());
    }

    #[test]
    fn negative_opcodes_require_a_failure() {
        assert!(check_unsupported_opcode(0x48, true).is_ok());
        assert_eq!(
            check_unsupported_opcode(0x48, false),
            Err(QuestViolation::UnsupportedOpcodeWithoutFailure { type_id: 0x48 })
        );
    }

    #[test]
    fn closing_reputation_has_three_shapes() {
        assert!(check_closing_reputation(0, 0).is_ok());
        assert!(check_closing_reputation(41, SUCCESS_REPUTATION).is_ok());
        assert!(check_closing_reputation(41, FAILURE_REPUTATION).is_ok());
        assert!(check_closing_reputation(41, 0).is_err());
        assert!(check_closing_reputation(0, SUCCESS_REPUTATION).is_err());
    }

    #[test]
    fn display_messages_never_enter_the_journal() {
        assert!(check_display_message_not_journaled(9, &[1, 2]).is_ok());
        assert_eq!(
            check_display_message_not_journaled(9, &[1, 9]),
            Err(QuestViolation::DisplayMessageJournaled { message_id: 9 })
        );
    }

    #[test]
    fn birth_state_and_missing_message_break() {
        assert!(check_task_birth_state(true, true).is_ok());
        assert!(check_task_birth_state(false, false).is_ok());
        assert_eq!(
            check_task_birth_state(true, false),
            Err(QuestViolation::TaskBirthStateMismatch {
                auto_start: true,
                is_triggered: false
            })
        );
        assert_eq!(
            check_task_birth_state(false, true),
            Err(QuestViolation::TaskBirthStateMismatch {
                auto_start: false,
                is_triggered: true
            })
        );
        assert!(check_missing_message_break(999, false, false).is_ok());
        assert!(check_missing_message_break(7, true, true).is_ok());
        assert_eq!(
            check_missing_message_break(999, false, true),
            Err(QuestViolation::MissingMessageBrokeTick { message_id: 999 })
        );
    }

    #[test]
    fn task_lifecycle_invariants() {
        assert!(check_persist_until("persist", "target", true, false).is_ok());
        assert!(check_persist_until("persist", "target", true, true).is_err());
        assert!(check_task_executed("t", false, true).is_ok());
        assert_eq!(
            check_task_executed("t", true, true),
            Err(QuestViolation::DroppedTaskExecuted {
                task: "t".to_string()
            })
        );
    }
}
