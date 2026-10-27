//! `UiShell` — the spine's UI shell (contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §5).
//!
//! Perception owns the **registry and the stack** of windows: which windows
//! exist, which are open, which one is on top and where an input goes.
//!
//! ## What it must NOT contain (§5 "Prohibido")
//!
//! Prices, HP and quests live in the read models of `perception::read_models`
//! (and behind them in `app`/the ECS). The shell **reads** nothing and
//! **possesses** nothing: a [`WindowSpec`] is an id and a title, and
//! [`UiOutcome`] carries ids only. There is no numeric game state in this
//! module, which is why "the shell owns the HUD's health" cannot happen by
//! construction rather than by discipline.
//!
//! The 85-window census (16 createchar + 11 config + 7 infra + 51 playable)
//! is **closed data** in [`crate::ui_census`] (spec M34 §1.1, GV-01/GV-02/
//! GV-19): [`WindowRegistry::canonical`] registers exactly those 85 entries,
//! so "the census is complete" and "the shell can open every window" are the
//! same fact (probe `UI-FULL-001` block A).

use std::fmt;

use daggerfall_nervous::events::WindowId;

use crate::ui_census;

/// One window the shell knows: identity and caption, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    /// Window identity.
    pub id: WindowId,
    /// Caption/asset key the presentation layer resolves.
    pub title: &'static str,
}

/// Base of the derived ids; keeps the four contract ids (0..=3) untouched.
const CANONICAL_BASE: u16 = 4;

/// Canonical [`WindowId`] of a census row.
///
/// The four spine panels that already travel on the bus keep their contract
/// ids (`WindowId::{HUD, INVENTORY, QUESTS, MAIN_MENU}`); the other 81 derive
/// theirs from the frozen census position (GV-02), so the id is a function of
/// the name's index in a digest-pinned list — never of registration order.
pub fn canonical_id(index: usize, name: &str) -> WindowId {
    match name {
        "DaggerfallHUD" => WindowId::HUD,
        "DaggerfallInventoryWindow" => WindowId::INVENTORY,
        "DaggerfallQuestJournalWindow" => WindowId::QUESTS,
        "DaggerfallStartWindow" => WindowId::MAIN_MENU,
        _ => WindowId::new(CANONICAL_BASE + index as u16),
    }
}

/// Canonical id of a census window by name, `None` when it is not in the
/// census (the caller decides whether that is a hard error).
pub fn canonical_id_for(name: &str) -> Option<WindowId> {
    ui_census::census()
        .iter()
        .position(|row| row.name == name)
        .map(|index| canonical_id(index, name))
}

/// Why a shell operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiError {
    /// The window was never registered.
    Unknown {
        /// Window asked for.
        id: WindowId,
    },
    /// The window is already registered.
    AlreadyRegistered {
        /// Window asked for.
        id: WindowId,
    },
    /// The window is already open.
    AlreadyOpen {
        /// Window asked for.
        id: WindowId,
    },
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { id } => write!(f, "window {} is not registered", id.index()),
            Self::AlreadyRegistered { id } => {
                write!(f, "window {} is already registered", id.index())
            }
            Self::AlreadyOpen { id } => write!(f, "window {} is already open", id.index()),
        }
    }
}

impl std::error::Error for UiError {}

/// The windows that exist (ids + captions, canonical registration order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowRegistry {
    windows: Vec<WindowSpec>,
}

impl WindowRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
        }
    }

    /// The canonical registry: exactly the 85 census windows (GV-01), in
    /// census order, caption = DFU class name.
    ///
    /// Propagates [`UiError::AlreadyRegistered`] instead of swallowing it: a
    /// duplicate would mean two census rows collapsed into one id, which is a
    /// census bug, not a runtime condition.
    pub fn canonical() -> Result<Self, UiError> {
        let mut registry = Self::new();
        for (index, row) in ui_census::census().iter().enumerate() {
            registry.register(canonical_id(index, row.name), row.name)?;
        }
        Ok(registry)
    }

    /// Registers a window. One id, one spec.
    pub fn register(&mut self, id: WindowId, title: &'static str) -> Result<(), UiError> {
        if self.contains(id) {
            return Err(UiError::AlreadyRegistered { id });
        }
        self.windows.push(WindowSpec { id, title });
        Ok(())
    }

    /// Is `id` known?
    pub fn contains(&self, id: WindowId) -> bool {
        self.windows.iter().any(|spec| spec.id == id)
    }

    /// Caption of `id`, if known.
    pub fn title(&self, id: WindowId) -> Option<&'static str> {
        self.windows
            .iter()
            .find(|spec| spec.id == id)
            .map(|spec| spec.title)
    }

    /// Registered windows, in registration order.
    pub fn specs(&self) -> &[WindowSpec] {
        &self.windows
    }

    /// How many windows are registered.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// No window registered.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

/// One routed input (§5 `route_input`).
///
/// The key bindings are not here: M34 maps device input to these two verbs
/// (the shell is not an input device — `m2_player` owns `ButtonInput`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInput {
    /// Accept on the top window.
    Confirm,
    /// Dismiss the top window.
    Cancel,
}

/// What the shell did with an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiOutcome {
    /// Nothing was open (or nothing to confirm): the input was dropped.
    Ignored,
    /// The top window accepted the input and stayed.
    Confirmed(WindowId),
    /// The top window was closed by the input.
    Closed(WindowId),
}

impl UiOutcome {
    /// Short label for evidence lines.
    pub fn label(self) -> String {
        match self {
            Self::Ignored => "Ignored".to_string(),
            Self::Confirmed(id) => format!("Confirmed({})", id.index()),
            Self::Closed(id) => format!("Closed({})", id.index()),
        }
    }
}

/// The shell: a registry of what exists plus a stack of what is open (§5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiShell {
    registry: WindowRegistry,
    stack: Vec<WindowId>,
}

impl UiShell {
    /// Shell over `registry`, every window closed.
    pub fn new(registry: WindowRegistry) -> Self {
        Self {
            registry,
            stack: Vec::new(),
        }
    }

    /// Shell over the canonical 85-window registry (census closed).
    pub fn canonical() -> Result<Self, UiError> {
        Ok(Self::new(WindowRegistry::canonical()?))
    }

    /// The registry (what exists).
    pub fn registry(&self) -> &WindowRegistry {
        &self.registry
    }

    /// Opens `id` on top of the stack.
    pub fn open(&mut self, id: WindowId) -> Result<(), UiError> {
        if !self.registry.contains(id) {
            return Err(UiError::Unknown { id });
        }
        if self.is_open(id) {
            return Err(UiError::AlreadyOpen { id });
        }
        self.stack.push(id);
        Ok(())
    }

    /// Closes `id` wherever it is in the stack (0 error: closing a closed
    /// window is a no-op, per §5's signature).
    pub fn close(&mut self, id: WindowId) {
        self.stack.retain(|open| *open != id);
    }

    /// Opens `id` when closed, closes it when open (§5 signature: infallible —
    /// an unknown id is a no-op).
    pub fn toggle(&mut self, id: WindowId) {
        if self.is_open(id) {
            self.close(id);
        } else if self.registry.contains(id) {
            self.stack.push(id);
        }
    }

    /// Window on top of the stack.
    pub fn top(&self) -> Option<WindowId> {
        self.stack.last().copied()
    }

    /// Is `id` open?
    pub fn is_open(&self, id: WindowId) -> bool {
        self.stack.contains(&id)
    }

    /// Open windows, bottom to top.
    pub fn stack(&self) -> &[WindowId] {
        &self.stack
    }

    /// How many windows are open.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Routes one input to the top window.
    pub fn route_input(&mut self, ev: UiInput) -> UiOutcome {
        match ev {
            UiInput::Confirm => match self.top() {
                Some(id) => UiOutcome::Confirmed(id),
                None => UiOutcome::Ignored,
            },
            UiInput::Cancel => match self.stack.pop() {
                Some(id) => UiOutcome::Closed(id),
                None => UiOutcome::Ignored,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> UiShell {
        let mut registry = WindowRegistry::new();
        for (id, title) in [
            (WindowId::HUD, "hud"),
            (WindowId::INVENTORY, "inventory"),
            (WindowId::QUESTS, "quests"),
            (WindowId::MAIN_MENU, "main-menu"),
        ] {
            registry.register(id, title).expect("register");
        }
        UiShell::new(registry)
    }

    #[test]
    fn the_four_current_panels_are_registrable() {
        let shell = shell();
        assert_eq!(shell.registry().len(), 4);
        assert_eq!(shell.registry().title(WindowId::QUESTS), Some("quests"));
        assert_eq!(shell.top(), None);
    }

    #[test]
    fn open_close_toggle_keep_the_stack_consistent() {
        let mut shell = shell();
        shell.open(WindowId::HUD).expect("hud");
        shell.open(WindowId::INVENTORY).expect("inventory");
        assert_eq!(shell.top(), Some(WindowId::INVENTORY));
        assert_eq!(shell.depth(), 2);

        shell.close(WindowId::HUD);
        assert_eq!(shell.stack(), &[WindowId::INVENTORY]);

        shell.toggle(WindowId::QUESTS);
        assert_eq!(shell.top(), Some(WindowId::QUESTS));
        shell.toggle(WindowId::QUESTS);
        assert_eq!(shell.top(), Some(WindowId::INVENTORY));
    }

    #[test]
    fn unknown_windows_are_refused_or_ignored() {
        let mut shell = shell();
        let ghost = WindowId::new(999);
        assert_eq!(
            shell.open(ghost),
            Err(UiError::Unknown { id: ghost })
        );
        shell.toggle(ghost);
        assert_eq!(shell.depth(), 0, "toggle of an unknown id is a no-op");
        shell.open(WindowId::HUD).expect("hud");
        assert_eq!(
            shell.open(WindowId::HUD),
            Err(UiError::AlreadyOpen { id: WindowId::HUD })
        );
    }

    #[test]
    fn input_routing_targets_the_top_window() {
        let mut shell = shell();
        assert_eq!(shell.route_input(UiInput::Confirm), UiOutcome::Ignored);
        assert_eq!(shell.route_input(UiInput::Cancel), UiOutcome::Ignored);

        shell.open(WindowId::HUD).expect("hud");
        shell.open(WindowId::QUESTS).expect("quests");
        assert_eq!(
            shell.route_input(UiInput::Confirm),
            UiOutcome::Confirmed(WindowId::QUESTS)
        );
        assert_eq!(
            shell.route_input(UiInput::Cancel),
            UiOutcome::Closed(WindowId::QUESTS)
        );
        assert_eq!(shell.top(), Some(WindowId::HUD));
    }

    #[test]
    fn a_duplicate_registration_is_refused() {
        let mut registry = WindowRegistry::new();
        registry.register(WindowId::HUD, "hud").expect("first");
        assert_eq!(
            registry.register(WindowId::HUD, "hud-again"),
            Err(UiError::AlreadyRegistered { id: WindowId::HUD })
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_canonical_registry_is_the_closed_85_with_unique_ids() {
        let registry = WindowRegistry::canonical().expect("canonical registry");
        assert_eq!(registry.len(), crate::ui_census::CENSUS_SIZE);
        assert_eq!(registry.len(), 85);

        let mut ids: Vec<u16> = registry
            .specs()
            .iter()
            .map(|spec| spec.id.index())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 85, "ids must be unique");

        for row in crate::ui_census::census() {
            let id = canonical_id_for(row.name).expect("census row has an id");
            assert_eq!(registry.title(id), Some(row.name));
        }
        assert_eq!(registry.title(WindowId::HUD), Some("DaggerfallHUD"));
        assert_eq!(
            registry.title(WindowId::INVENTORY),
            Some("DaggerfallInventoryWindow")
        );
        assert_eq!(
            registry.title(WindowId::QUESTS),
            Some("DaggerfallQuestJournalWindow")
        );
        assert_eq!(
            registry.title(WindowId::MAIN_MENU),
            Some("DaggerfallStartWindow")
        );
    }

    #[test]
    fn every_census_window_opens_and_closes_leaving_depth_zero() {
        let mut shell = UiShell::canonical().expect("canonical shell");
        for row in crate::ui_census::census() {
            let id = canonical_id_for(row.name).expect("census row has an id");
            shell.open(id).expect("open");
            assert_eq!(shell.top(), Some(id));
            shell.close(id);
            assert_eq!(shell.depth(), 0, "{} must close cleanly", row.name);
            assert!(!shell.is_open(id));
        }
        assert_eq!(canonical_id_for("NotAWindow"), None);
    }
}
