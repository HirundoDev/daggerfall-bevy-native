//! **M-QST · la quest principal y su primer acto** (`CR-9b3e5d70`): estado y
//! tabla legal de transiciones, sin ECS, sin I/O y sin relojes.
//!
//! ## Por qué existe este módulo
//!
//! El disparador y el acto 1 son **reglas**, no presentación: qué acciones
//! acepta cada estado y cuál es el destino. Eso se decide aquí, puro, y la
//! ventana (`m18_window`) y la ruta sin ventana (`mainquest_route`) solo
//! traducen teclas y mueren de un enemigo a las entradas de la tabla. Sin esta
//! separación cada consumidor tendría su propia copia del grafo y dos
//! consumidores podrían aceptar aristas distintas — el defecto que la tabla
//! única hace imposible.
//!
//! ## Una sola puerta
//!
//! [`apply`] es la **única** función que produce un estado nuevo. No hay
//! mutadores sueltos (`set_state`, `accept()`, `advance()`…): cualquier
//! escritura del estado pasa por la tabla de [`TRANSITIONS`], y cualquier par
//! que no esté en ella devuelve [`Refusal`] **con el estado intacto**. En
//! particular `Declined` no es «aceptada»: sin `Accept` el acto no puede
//! completarse.
//!
//! ## Las 7 aristas (contrato heredado, `HH-20261031-mquesttrigger-6d21b8e7`)
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
//! Todo lo demás (35 pares de los 42 posibles) es [`Refusal`].
//!
//! ## Persistencia
//!
//! El estado viaja al save v3 como **una fila de texto** en
//! `QuestsSection.states` (`MQ-01.state=<token>`), que es la forma que el
//! documento ya modela: 0 bump de versión, 0 migración, 0 sección nueva. El
//! token es parte del contrato de datos ([`State::token`]) y no cambia sin
//! migración: [`STATE_KEY`] + token son la identidad persistida.

use std::fmt;

/// Identificador de la quest en el registro de quests del save.
pub const QUEST_ID: &str = "MQ-01";

/// Clave de la fila del estado en `QuestsSection::states`.
///
/// El documento no admite claves repetidas de sección, así que la quest
/// escribe **una** fila con clave propia; el valor es el token del estado.
pub const STATE_KEY: &str = "MQ-01.state";

/// Estados del ciclo de vida de la quest principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum State {
    /// Nadie ha ofrecido la quest todavía (estado inicial de una sesión).
    #[default]
    Unstarted,
    /// La oferta está en pantalla y espera `Y`/`N`.
    Offered,
    /// Acto 1 en curso: hay que matar al objetivo.
    Act1Hunt,
    /// El objetivo cayó: hay que entregar la prueba.
    Act1Return,
    /// Acto 1 cerrado y entregado.
    Act1Done,
    /// La oferta fue rechazada: no se repite en esta sesión ni en el save.
    Declined,
    /// El acto se abandonó: terminal, sin retorno.
    Abandoned,
}

impl State {
    /// Todos los estados, en orden de ciclo de vida.
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

    /// Reconstruye un estado desde su token; `None` si el token no es del
    /// contrato (un documento de otra época no se adivina).
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.token() == token)
    }

    /// ¿El acto 1 está en curso (aceptado y sin cerrar)?
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Act1Hunt | Self::Act1Return)
    }

    /// ¿El ciclo no admite más acciones?
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Act1Done | Self::Declined | Self::Abandoned)
    }

    /// ¿La oferta ya se emitió y sigue viva en pantalla o en curso?
    pub const fn is_offered(self) -> bool {
        !matches!(self, Self::Unstarted)
    }
}

/// Acciones que un consumidor puede pedir a la máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// El disparador ofrece la quest (solo desde `Unstarted`).
    Offer,
    /// El jugador acepta (`Y`).
    Accept,
    /// El jugador rechaza (`N`).
    Decline,
    /// El objetivo del acto 1 murió.
    Slay,
    /// La prueba se entrega (acto 1 completo).
    Complete,
    /// El jugador abandona el acto (`X`).
    Abandon,
}

impl Action {
    /// Todas las acciones, en el orden de la tabla.
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

/// Las 7 aristas legales, en el orden del contrato heredado.
pub const TRANSITIONS: [(State, Action, State); 7] = [
    (State::Unstarted, Action::Offer, State::Offered),
    (State::Offered, Action::Accept, State::Act1Hunt),
    (State::Offered, Action::Decline, State::Declined),
    (State::Act1Hunt, Action::Slay, State::Act1Return),
    (State::Act1Hunt, Action::Abandon, State::Abandoned),
    (State::Act1Return, Action::Complete, State::Act1Done),
    (State::Act1Return, Action::Abandon, State::Abandoned),
];

/// Rechazo tipado de una arista que la tabla no declara.
///
/// Lleva el par pedido para que el consumidor pueda publicarlo: un rechazo
/// silencioso sería indistinguible de una acción que nadie pulsó.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// Estado desde el que se intentó la acción (sin cambios).
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

/// **La única puerta**: aplica `action` sobre `from` y devuelve el estado
/// nuevo, o el [`Refusal`] del par que la tabla no declara.
///
/// `from` entra por valor y no se muta: un rechazo no puede dejar el estado
/// a medias, porque no hay nada que dejar a medias — el llamante conserva el
/// suyo y decide qué publicar.
pub fn apply(from: State, action: Action) -> Result<State, Refusal> {
    TRANSITIONS
        .iter()
        .find(|(edge_from, edge_action, _)| *edge_from == from && *edge_action == action)
        .map(|(_, _, to)| *to)
        .ok_or(Refusal { from, action })
}

/// ¿La tabla declara esta arista? (consulta pura; no sustituye a [`apply`]).
pub fn is_legal(from: State, action: Action) -> bool {
    apply(from, action).is_ok()
}

/// Acciones legales desde `from`, en el orden de la tabla.
pub fn legal_actions(from: State) -> impl Iterator<Item = Action> {
    TRANSITIONS
        .iter()
        .filter(move |(edge_from, _, _)| *edge_from == from)
        .map(|(_, action, _)| *action)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La tabla tiene exactamente 7 aristas y ninguna repite el par
    /// `(from, action)` — un par duplicado haría que el orden de la tabla
    /// decidiera el destino.
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

    /// Matriz exhaustiva: 7 estados × 6 acciones = 42 pares; exactamente los 7
    /// de la tabla pasan y los otros 35 devuelven `Refusal` con el par pedido.
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

    /// Tokens: ida y vuelta para los 7 estados, y un token ajeno no se adivina.
    #[test]
    fn state_tokens_round_trip_and_unknown_tokens_are_rejected() {
        for state in State::ALL {
            assert_eq!(State::from_token(state.token()), Some(state));
        }
        assert_eq!(State::from_token(""), None);
        assert_eq!(State::from_token("Act1Hunt"), None, "tokens are data, not Debug");
    }

    /// Falsificador (a): `Offered --Complete-->` no existe. Si la tabla
    /// tuviera esa arista, este test caería.
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

    /// Falsificador (c): `Declined` no es «aceptada». Sin `Accept` el acto no
    /// puede completarse: desde `Declined` solo hay rechazos, y el único
    /// camino a `Act1Done` exige pasar por `Act1Hunt`/`Act1Return`.
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
        // El único ingreso al acto es `Accept` desde `Offered`.
        let entries: Vec<State> = TRANSITIONS
            .iter()
            .filter(|(_, action, to)| *action == Action::Accept && *to == State::Act1Hunt)
            .map(|(from, _, _)| *from)
            .collect();
        assert_eq!(entries, vec![State::Offered]);
    }

    /// La consulta de acciones legales coincide con la tabla, estado por
    /// estado (si las dos fuentes divergieran, el consumidor ofrecería una
    /// tecla que la puerta rechaza).
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

    /// Un terminal no acepta nada: `is_terminal` es la lectura del mismo grafo.
    #[test]
    fn terminal_states_have_no_outgoing_edge() {
        for state in State::ALL {
            assert_eq!(state.is_terminal(), legal_actions(state).next().is_none());
        }
    }
}
