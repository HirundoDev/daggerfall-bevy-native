//! The scene-transition law types, **relocated** from `app` (plan §3.1.1 row
//! `SceneTransition`: "hoy es ley de facto atrapada en el binario").
//!
//! Contract source: `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md`
//! §4. The five items keep their exact signatures and semantics: this module
//! moved the types, it did not change them. `app::interior_transit` re-exports
//! them, so every existing consumer keeps its path
//! (`app::interior_transit::SceneTransition`) with 0 duplicated definitions —
//! the same pattern the module already used for `SceneTarget`.
//!
//! What stays in `app`: the door registry (`DoorSite` / `DoorRegistry`), the
//! `SceneLoader` trait and the transit orchestrator — those wire *this* game's
//! geometry and are not contract vocabulary.

use std::fmt;

use daggerfall_nervous::events::{SceneId, SceneTarget};

/// Why a transition could not be requested or completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneError {
    /// A target is already queued (the contract's queue holds one).
    Busy(SceneTarget),
    /// No door with that id exists in the registry.
    UnknownDoor {
        /// Requested door id.
        door: u32,
    },
    /// The player is farther than the door's reach.
    NotAdjacent {
        /// Door id that was tried.
        door: u32,
        /// Measured distance in millimetres (integer: deterministic).
        distance_mm: u32,
    },
    /// The transition was asked to leave a scene it is not in.
    NotInScene {
        /// Scene required by the operation.
        expected: SceneId,
        /// Scene the transit is actually in.
        found: SceneId,
    },
    /// The loader does not hold that block.
    BlockNotFound {
        /// Block index requested.
        block: u32,
    },
    /// The loader refused the target (assembly error, missing content, …).
    LoadRejected {
        /// Block/record involved.
        block: u32,
        /// Loader reason, verbatim.
        reason: String,
    },
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy(target) => write!(f, "transition busy: {} already queued", target.label()),
            Self::UnknownDoor { door } => write!(f, "unknown door {door}"),
            Self::NotAdjacent { door, distance_mm } => write!(
                f,
                "door {door} out of reach ({:.2} m)",
                *distance_mm as f32 / 1000.0
            ),
            Self::NotInScene { expected, found } => {
                write!(f, "not in {} (found {})", expected.label(), found.label())
            }
            Self::BlockNotFound { block } => write!(f, "block {block} not loaded"),
            Self::LoadRejected { block, reason } => {
                write!(f, "block {block} rejected by the loader: {reason}")
            }
        }
    }
}

impl std::error::Error for SceneError {}

/// State of the transition (contract §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionState {
    /// Nothing in flight.
    Idle,
    /// `app` is loading this target.
    Loading(SceneTarget),
    /// Load finished. `poll()` hands this value out exactly once.
    Done(SceneTarget),
    /// Load (or validation) failed; the scene did not change.
    Failed(SceneError),
}

impl TransitionState {
    /// Target of a `Loading`/`Done` state.
    pub const fn target(&self) -> Option<SceneTarget> {
        match self {
            Self::Loading(target) | Self::Done(target) => Some(*target),
            Self::Idle | Self::Failed(_) => None,
        }
    }

    /// Short label for evidence lines.
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Idle".to_string(),
            Self::Loading(target) => format!("Loading({})", target.label()),
            Self::Done(target) => format!("Done({})", target.label()),
            Self::Failed(error) => format!("Failed({error})"),
        }
    }
}

/// Scene transition (contract §4).
///
/// `request` validates and enqueues (queue of ONE); `poll` hands `Done` out
/// **once**; the load itself belongs to `app` (`begin` → loader → `complete`
/// or `fail`).
///
/// `pending` carries two roles with no extra field, exactly as the contract
/// describes the struct: while `Loading` it is the queued target (the "cola 1"
/// guard), and while `Done` it is the delivery ticket that `poll` consumes —
/// which is what makes `Done` a single-shot value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneTransition {
    state: TransitionState,
    pending: Option<SceneTarget>,
}

impl Default for SceneTransition {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneTransition {
    /// Fresh transition (`Idle`).
    pub fn new() -> Self {
        Self {
            state: TransitionState::Idle,
            pending: None,
        }
    }

    /// Validates and enqueues ONE target.
    ///
    /// The queue holds ONE *load*: a new request is rejected while a load is in
    /// flight. A `Done` that was never polled is superseded by the next
    /// request (it describes the transition that just finished); `poll` keeps
    /// its single-shot contract for whoever observes it.
    pub fn request(&mut self, target: SceneTarget) -> Result<(), SceneError> {
        if let TransitionState::Loading(loading) = self.state {
            return Err(SceneError::Busy(loading));
        }
        self.pending = Some(target);
        self.state = TransitionState::Loading(target);
        Ok(())
    }

    /// Hands out the current state; `Done` is delivered exactly once.
    pub fn poll(&mut self) -> Option<TransitionState> {
        match self.state.clone() {
            TransitionState::Idle => None,
            TransitionState::Loading(target) => Some(TransitionState::Loading(target)),
            TransitionState::Failed(error) => Some(TransitionState::Failed(error)),
            TransitionState::Done(target) => {
                if self.pending.take().is_some() {
                    Some(TransitionState::Done(target))
                } else {
                    None
                }
            }
        }
    }

    /// Current state (`Done` keeps its value here after `poll` consumed it).
    pub fn state(&self) -> &TransitionState {
        &self.state
    }

    /// The target `app` must load, if a load is in flight (R-8 addendum).
    pub fn begin(&mut self) -> Option<SceneTarget> {
        match self.state {
            TransitionState::Loading(target) => Some(target),
            _ => None,
        }
    }

    /// Reports a finished load (R-8 addendum). Makes `Done` deliverable once.
    pub fn complete(&mut self, target: SceneTarget) -> Result<(), SceneError> {
        match self.state {
            TransitionState::Loading(loading) if loading == target => {
                self.state = TransitionState::Done(target);
                self.pending = Some(target);
                Ok(())
            }
            TransitionState::Loading(loading) => Err(SceneError::LoadRejected {
                block: u32::MAX,
                reason: format!(
                    "completed {} while {} was in flight",
                    target.label(),
                    loading.label()
                ),
            }),
            _ => Err(SceneError::LoadRejected {
                block: u32::MAX,
                reason: format!("no load in flight for {}", target.label()),
            }),
        }
    }

    /// Reports a failed load/validation (R-8 addendum). The scene does not
    /// change; the failure stays readable through [`Self::state`] and
    /// [`Self::poll`].
    pub fn fail(&mut self, error: SceneError) {
        self.state = TransitionState::Failed(error);
        self.pending = None;
    }
}
