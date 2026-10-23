//! Antibodies organ — invariants and guardrails.
//!
//! The "no Bethesda bytes tracked by git" invariant is enforced today by
//! `.gitignore` + the negative verification in DATA-CLASSIFICATION.md.
//!
//! M28: [`crime_guardrails`] — typed invariants of the law domain
//! (REQ-L99-061): legal reputation inside `[-100, 100]`, valid crime values,
//! non-negative sentences, the fine never above the gold, the split identity
//! when the gold cap does not bind, the prison clock (`days * 86400` plus
//! `14400` on release) and the guard quota of five.
//!
//! M29: [`quest_guardrails`] — typed invariants of the quest machine
//! (REQ-L99-062): non-negative clocks, `ticksToEnd` in `{0,1,2}`, unique
//! journal steps, a missing message never counted as success, a NEGATIVE
//! opcode never silent, closing reputation in `{0,+5,-2}`, dropped tasks
//! never executed, no notes in the finished journal, no display-only
//! message (`say`, type `0x33`) journaled, a task triggered at birth only
//! when its kind starts on its own (`Headless`) and a missing message never
//! breaking the tick.

pub mod crime_guardrails;
pub mod quest_guardrails;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "antibodies";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
