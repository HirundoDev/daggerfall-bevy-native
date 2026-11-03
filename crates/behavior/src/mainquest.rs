//! **M-QST · the main quest and its first act** (`CR-9b3e5d70`): state and
//! legal transition table, no ECS, no I/O and no clocks.
//!
//! ## Why this module exists
//!
//! The trigger and act 1 are **rules**, not presentation: which actions
//! each state accepts and where each one leads. That is decided here, pure,
//! and the window (`m18_window`) and the windowless route (`mainquest_route`)
//! only translate keys and enemy deaths into entries of the table. Without
//! this separation every consumer would keep its own copy of the graph and
//! two consumers could accept different edges — the defect the single table
//! makes impossible.
//!
//! ## A single door
//!
//! [`apply`] is the **only** function that produces a new state. There are no
//! loose mutators (`set_state`, `accept()`, `advance()`…): any write of the
//! state goes through the [`TRANSITIONS`] table, and any pair that is not in
//! it returns [`Refusal`] **with the state untouched**. In particular
//! `Declined` does not mean accepted: without `Accept` the act cannot
//! complete.
//!
//! ## The 7 edges (inherited contract, `HH-20261031-mquesttrigger-6d21b8e7`)
//!
//! ```text
//! Unstarted --Offer----> Offered
//! Offered   --Accept---> Act1Hunt
//! Offered   --Decline--> Declined
//! Act1Hunt  --Slay-----> Act1Return
//! Act1Hunt  --Abandon--> Abandoned
//! Act1Return--Complete-> Act1Done
//! Act1Return--Abandon--> Abandoned
//! ```
//!
//! Everything else (35 of the 42 possible pairs) is [`Refusal`].
//!
//! ## Persistence
//!
//! The state travels to the v3 save as **one text row** in
//! `QuestsSection.states` (`MQ-01.state=<token>`), which is the shape the
//! document already models: 0 version bump, 0 migration, 0 new section. The
//! token is part of the data contract ([`State::token`]) and does not change
//! without a migration: [`STATE_KEY`] + token are the persisted identity.

use std::fmt;

/// Identifier of the quest in the save's quest registry.
pub const QUEST_ID: &str = "MQ-01";

/// Key of the state row in `QuestsSection::states`.
///
/// The document rejects repeated section keys, so the quest writes **one**
/// row with its own key; the value is the state token.
pub const STATE_KEY: &str = "MQ-01.state";

/// Lifecycle states of the main quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum State {
    /// Nobody has offered the quest yet (initial state of a session).
    #[default]
    Unstarted,
    /// The offer is on screen and waits for `Y`/`N`.
    Offered,
    /// Act 1 in progress: the target has to die.
    Act1Hunt,
    /// The target fell: the proof has to be delivered.
    Act1Return,
    /// Acto 1 cerrado y entregado.
    Act1Done,
    /// The offer was declined: it does not repeat in this session nor in the save.
    Declined,
    /// El acto se abandonó: terminal, sin retorno.
    Abandoned,
}

impl State {
    /// All states, in lifecycle order.
    pub const ALL: [Self; 7] = [
        Self::Unstarted,
        Self::Offered,
        Self::Act1Hunt,
        Self::Act1Return,
        Self::Act1Done,
        Self::Declined,
        Self::Abandoned,
    ];

    /// Token persistido en `QuestsSection.states` (contrato de datos).
    pub const fn token(self) -> &'static str {
        match self {
            Self::Unstarted => "unstarted",
            Self::Offered => "offered",
            Self::Act1Hunt => "act1-hunt",
            Self::Act1Return => "act1-return",
            Self::Act1Done => "act1-done",
            Self::Declined => "declined",
            Self::Abandoned => "abandoned",
        }
    }

    /// Rebuilds a state from its token; `None` if the token is not part of the
    /// contract (a document from another era is never guessed).
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.token() == token)
    }

    /// Is act 1 in progress (accepted and not closed)?
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Act1Hunt | Self::Act1Return)
    }

    /// Does the cycle accept no further actions?
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Act1Done | Self::Declined | Self::Abandoned)
    }

    /// Was the offer already emitted and is it still on screen or in progress?
    pub const fn is_offered(self) -> bool {
        !matches!(self, Self::Unstarted)
    }
}

/// Actions a consumer may ask the machine to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// The trigger offers the quest (only from `Unstarted`).
    Offer,
    /// El jugador acepta (`Y`).
    Accept,
    /// El jugador rechaza (`N`).
    Decline,
    /// The act 1 target died.
    Slay,
    /// La prueba se entrega (acto 1 completo).
    Complete,
    /// The player abandons the act (`X`).
    Abandon,
}

impl Action {
    /// All actions, in the order of the table.
    pub const ALL: [Self; 6] = [
        Self::Offer,
        Self::Accept,
        Self::Decline,
        Self::Slay,
        Self::Complete,
        Self::Abandon,
    ];

    /// Token estable para evidencia y tests.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Offer => "offer",
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Slay => "slay",
            Self::Complete => "complete",
            Self::Abandon => "abandon",
        }
    }
}

/// The 7 legal edges, in the order of the inherited contract.
pub const TRANSITIONS: [(State, Action, State); 7] = [
    (State::Unstarted, Action::Offer, State::Offered),
    (State::Offered, Action::Accept, State::Act1Hunt),
    (State::Offered, Action::Decline, State::Declined),
    (State::Act1Hunt, Action::Slay, State::Act1Return),
    (State::Act1Hunt, Action::Abandon, State::Abandoned),
    (State::Act1Return, Action::Complete, State::Act1Done),
    (State::Act1Return, Action::Abandon, State::Abandoned),
];

/// Typed refusal of an edge the table does not declare.
///
/// It carries the requested pair so the consumer can publish it: a silent
/// refusal would be indistinguishable from an action nobody pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// State from which the action was attempted (unchanged).
    pub from: State,
    /// Acción rechazada.
    pub action: Action,
}

impl fmt::Display for Refusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "action `{}` is not legal from `{}`",
            self.action.token(),
            self.from.token()
        )
    }
}

impl std::error::Error for Refusal {}

/// **The single door**: applies `action` to `from` and returns the new
/// state, or the [`Refusal`] of the pair the table does not declare.
///
/// `from` comes in by value and is never mutated: a refusal cannot leave the
/// state half-applied, because there is nothing to half-apply — the caller
/// keeps its own and decides what to publish.
pub fn apply(from: State, action: Action) -> Result<State, Refusal> {
    TRANSITIONS
        .iter()
        .find(|(edge_from, edge_action, _)| *edge_from == from && *edge_action == action)
        .map(|(_, _, to)| *to)
        .ok_or(Refusal { from, action })
}

/// Does the table declare this edge? (pure query; it does not replace [`apply`]).
pub fn is_legal(from: State, action: Action) -> bool {
    apply(from, action).is_ok()
}

/// Legal actions from `from`, in the order of the table.
pub fn legal_actions(from: State) -> impl Iterator<Item = Action> {
    TRANSITIONS
        .iter()
        .filter(move |(edge_from, _, _)| *edge_from == from)
        .map(|(_, action, _)| *action)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table holds exactly 7 edges and none repeats the pair
    /// `(from, action)` — a duplicated pair would let the order of the table
    /// decide the destination.
    #[test]
    fn the_table_has_seven_unique_edges() {
        assert_eq!(TRANSITIONS.len(), 7);
        let mut seen = std::collections::HashSet::new();
        for (from, action, to) in TRANSITIONS {
            assert!(
                seen.insert((from, action)),
                "duplicate edge {from:?} --{action:?}--> {to:?}"
            );
            assert_eq!(apply(from, action), Ok(to));
        }
    }

    /// Exhaustive matrix: 7 states × 6 actions = 42 pairs; exactly the 7 in the
    /// table pass and the other 35 return `Refusal` with the requested pair.
    #[test]
    fn the_matrix_is_exactly_the_table() {
        let legal: Vec<(State, Action)> = TRANSITIONS
            .iter()
            .map(|(from, action, _)| (*from, *action))
            .collect();
        let mut accepted = 0;
        for from in State::ALL {
            for action in Action::ALL {
                match apply(from, action) {
                    Ok(to) => {
                        accepted += 1;
                        assert!(
                            legal.contains(&(from, action)),
                            "{from:?} --{action:?}--> {to:?} is not in the table"
                        );
                    }
                    Err(refusal) => {
                        assert_eq!((refusal.from, refusal.action), (from, action));
                        assert!(
                            !legal.contains(&(from, action)),
                            "{from:?} --{action:?}--> is legal but was refused"
                        );
                    }
                }
            }
        }
        assert_eq!(accepted, 7);
        assert_eq!(accepted + 35, State::ALL.len() * Action::ALL.len());
    }

    /// Tokens: round trip for the 7 states, and a foreign token is not guessed.
    #[test]
    fn state_tokens_round_trip_and_unknown_tokens_are_rejected() {
        for state in State::ALL {
            assert_eq!(State::from_token(state.token()), Some(state));
        }
        assert_eq!(State::from_token(""), None);
        assert_eq!(State::from_token("Act1Hunt"), None, "tokens are data, not Debug");
    }

    /// Falsifier (a): `Offered --Complete-->` does not exist. If the table
    /// had that edge, this test would fall.
    #[test]
    fn falsifier_a_offered_cannot_complete() {
        let refusal = apply(State::Offered, Action::Complete).unwrap_err();
        assert_eq!(
            refusal,
            Refusal {
                from: State::Offered,
                action: Action::Complete
            }
        );
        // Y ninguna acción desde `Offered` lleva a `Act1Done`.
        for action in Action::ALL {
            assert_ne!(apply(State::Offered, action), Ok(State::Act1Done));
        }
    }

    /// Falsifier (c): `Declined` is not "accepted". Without `Accept` the act
    /// cannot complete: from `Declined` there are only refusals, and the only
    /// path to `Act1Done` goes through `Act1Hunt`/`Act1Return`.
    #[test]
    fn falsifier_c_declined_cannot_be_completed_without_accepting() {
        for action in Action::ALL {
            let _ = apply(State::Declined, action).unwrap_err();
        }
        assert_eq!(
            apply(State::Declined, Action::Slay),
            Err(Refusal {
                from: State::Declined,
                action: Action::Slay
            })
        );
        // The only entry into the act is `Accept` from `Offered`.
        let entries: Vec<State> = TRANSITIONS
            .iter()
            .filter(|(_, action, to)| *action == Action::Accept && *to == State::Act1Hunt)
            .map(|(from, _, _)| *from)
            .collect();
        assert_eq!(entries, vec![State::Offered]);
    }

    /// The legal-action query matches the table, state by state (if the two
    /// sources diverged, the consumer would offer a key that the door
    /// refuses).
    #[test]
    fn legal_actions_agree_with_the_table() {
        for from in State::ALL {
            let listed: Vec<Action> = legal_actions(from).collect();
            let expected: Vec<Action> = TRANSITIONS
                .iter()
                .filter(|(edge_from, _, _)| *edge_from == from)
                .map(|(_, action, _)| *action)
                .collect();
            assert_eq!(listed, expected, "state {from:?}");
            for action in Action::ALL {
                assert_eq!(listed.contains(&action), is_legal(from, action));
            }
        }
    }

    /// A terminal state accepts nothing: `is_terminal` is a read of the same graph.
    #[test]
    fn terminal_states_have_no_outgoing_edge() {
        for state in State::ALL {
            assert_eq!(state.is_terminal(), legal_actions(state).next().is_none());
        }
    }
}
