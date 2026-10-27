//! Lifecycle state and the control transition table.
//!
//! The table is total: every `(state, command)` pair either yields a next
//! state or a typed [`RpcError::IllegalTransition`] carrying `from` and
//! `command`. `stop` is deliberately **total and idempotent**: a stop that
//! arrives while the instance is already stopped succeeds and leaves it
//! stopped, which is the mechanical answer to "un stop no puede dejar el
//! proceso zombi" (plan 3.3).

use std::fmt;

use daggerfall_content_api::SideId;

use crate::error::RpcError;

/// One observed lifecycle state of the embedded instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// Not running.
    Stopped,
    /// Running and processing.
    Running,
    /// Running but suspended.
    Paused,
}

impl LifecycleState {
    /// Stable lowercase token used on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleState::Stopped => "stopped",
            LifecycleState::Running => "running",
            LifecycleState::Paused => "paused",
        }
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A control command a client may send.
///
/// The side-carrying variants hold a validated [`SideId`]; there is no variant
/// that accepts a filesystem path or an executable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlCommand {
    /// Enter `Running` from `Stopped`.
    Start,
    /// Enter `Stopped` from any state (idempotent).
    Stop,
    /// Enter `Paused` from `Running`.
    Pause,
    /// Enter `Running` from `Paused`.
    Resume,
    /// Load a side (only while `Running`).
    LoadSide(SideId),
    /// Unload a side (only while `Running`).
    UnloadSide(SideId),
}

impl ControlCommand {
    /// Stable kebab-case token used as the method name and in error payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlCommand::Start => "start",
            ControlCommand::Stop => "stop",
            ControlCommand::Pause => "pause",
            ControlCommand::Resume => "resume",
            ControlCommand::LoadSide(_) => "load-side",
            ControlCommand::UnloadSide(_) => "unload-side",
        }
    }

    /// The side this command acts on, if any.
    pub fn side(&self) -> Option<&SideId> {
        match self {
            ControlCommand::LoadSide(side) | ControlCommand::UnloadSide(side) => Some(side),
            ControlCommand::Start
            | ControlCommand::Stop
            | ControlCommand::Pause
            | ControlCommand::Resume => None,
        }
    }
}

impl fmt::Display for ControlCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Applies the transition table.
///
/// | Command | Legal from | Next state |
/// |:---|:---|:---|
/// | `start` | `stopped` | `running` |
/// | `pause` | `running` | `paused` |
/// | `resume` | `paused` | `running` |
/// | `stop` | `stopped`, `running`, `paused` | `stopped` (idempotent) |
/// | `load-side` | `running` | `running` |
/// | `unload-side` | `running` | `running` |
///
/// Every other pair is [`RpcError::IllegalTransition`].
pub fn transition(
    state: LifecycleState,
    command: &ControlCommand,
) -> Result<LifecycleState, RpcError> {
    let next = match (state, command) {
        (LifecycleState::Stopped, ControlCommand::Start) => LifecycleState::Running,
        (LifecycleState::Running, ControlCommand::Pause) => LifecycleState::Paused,
        (LifecycleState::Paused, ControlCommand::Resume) => LifecycleState::Running,
        // `stop` is total: from any state, including `stopped`, it is `Ok`.
        (_, ControlCommand::Stop) => LifecycleState::Stopped,
        (LifecycleState::Running, ControlCommand::LoadSide(_))
        | (LifecycleState::Running, ControlCommand::UnloadSide(_)) => state,
        _ => {
            return Err(RpcError::IllegalTransition {
                from: state.as_str(),
                command: command.as_str(),
            })
        }
    };
    Ok(next)
}
