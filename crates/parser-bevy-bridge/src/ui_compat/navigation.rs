//! A5 — navigation axis: a typed window stack over the perception shell
//! (plan §3.5).
//!
//! **The rule:** windows are addressed by [`WindowId`] only. There is no
//! `open("Inventory")` entry point — a string would let a typo create a second
//! window that the census never registered, which is exactly what the M36
//! audit found in the pre-M37 call sites.
//!
//! **This module does not edit perception.** [`UiShell`] is wrapped as-is, and
//! the one semantic it lacks is added here: `UiShell::close` is a **silent
//! no-op** on an absent window, so [`Navigation::close`] checks `is_open`
//! first and returns [`UiCompatError::Navigation`] instead of pretending the
//! window was closed. A leak that is swallowed at close time resurfaces as an
//! unowned surface three frames later; the axis refuses the silence at the
//! source.
//!
//! The open/close order is the painting order of the shell, and `depth()` /
//! `top()` are read straight from it, so a duplicate open, a phantom close or a
//! reordered stack is observable from outside without reaching into the shell.

use daggerfall_nervous::events::WindowId;
use daggerfall_perception::ui_shell::{UiError, UiShell, WindowRegistry};

use super::error::{
    UiCompatError, RULE_CLOSE_ABSENT, RULE_OPEN_DUPLICATE, RULE_SHELL_REFUSED, RULE_WINDOW_TYPED,
};

/// The UI navigation stack: which windows are open, in painting order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    shell: UiShell,
}

impl Navigation {
    /// Wraps an existing shell (used by tests and by the app, which already
    /// owns one).
    pub fn new(shell: UiShell) -> Self {
        Self { shell }
    }

    /// The canonical 85-window shell of the census.
    pub fn canonical() -> Result<Self, UiCompatError> {
        UiShell::canonical()
            .map(Self::new)
            .map_err(|_| UiCompatError::Navigation(RULE_SHELL_REFUSED))
    }

    /// Opens `window` on top of the stack.
    ///
    /// Refuses an unregistered id (rule 1) and a window that is already open
    /// (rule 3) before touching the shell.
    pub fn open(&mut self, window: WindowId) -> Result<(), UiCompatError> {
        if !self.shell.registry().contains(window) {
            return Err(UiCompatError::Navigation(RULE_WINDOW_TYPED));
        }
        if self.shell.is_open(window) {
            return Err(UiCompatError::Navigation(RULE_OPEN_DUPLICATE));
        }
        self.shell.open(window).map_err(map_shell_error)
    }

    /// Closes `window`, which must be open (rule 2).
    pub fn close(&mut self, window: WindowId) -> Result<(), UiCompatError> {
        if !self.shell.is_open(window) {
            return Err(UiCompatError::Navigation(RULE_CLOSE_ABSENT));
        }
        self.shell.close(window);
        Ok(())
    }

    /// Number of open windows.
    pub fn depth(&self) -> usize {
        self.shell.depth()
    }

    /// Window on top of the stack (the one that receives input).
    pub fn top(&self) -> Option<WindowId> {
        self.shell.top()
    }

    /// `true` when `window` is open.
    pub fn is_open(&self, window: WindowId) -> bool {
        self.shell.is_open(window)
    }

    /// Open windows in painting order (bottom to top).
    pub fn stack(&self) -> &[WindowId] {
        self.shell.stack()
    }

    /// The wrapped registry (titles, census names).
    pub fn registry(&self) -> &WindowRegistry {
        self.shell.registry()
    }

    /// Caption of a registered window.
    pub fn title(&self, window: WindowId) -> Option<&'static str> {
        self.shell.registry().title(window)
    }
}

fn map_shell_error(error: UiError) -> UiCompatError {
    match error {
        UiError::Unknown { .. } => UiCompatError::Navigation(RULE_WINDOW_TYPED),
        UiError::AlreadyOpen { .. } => UiCompatError::Navigation(RULE_OPEN_DUPLICATE),
        UiError::AlreadyRegistered { .. } => UiCompatError::Navigation(RULE_SHELL_REFUSED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A5 rule 2 — closing a window that is not open is an error.
    ///
    /// Violation caught: delegating straight to `UiShell::close`, which is a
    /// silent no-op on an absent window; the call would look successful and the
    /// stack would stay wrong.
    #[test]
    fn falsifier_a5_close_of_absent_is_error() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        assert_eq!(
            nav.close(WindowId::QUESTS),
            Err(UiCompatError::Navigation(RULE_CLOSE_ABSENT))
        );
        assert_eq!(nav.depth(), 0);

        nav.open(WindowId::QUESTS).expect("open quests");
        assert!(nav.is_open(WindowId::QUESTS));
        nav.close(WindowId::QUESTS).expect("close quests");
        assert!(!nav.is_open(WindowId::QUESTS));
        assert_eq!(
            nav.close(WindowId::QUESTS),
            Err(UiCompatError::Navigation(RULE_CLOSE_ABSENT))
        );
    }

    /// A5 rule 1 — no string window parameter anywhere in the axis.
    ///
    /// Violation caught: an `open_by_name(&str)` / `close("Inventory")` style
    /// entry point (which would bypass the registry and let a typo open a
    /// window the census never registered). The scan is source-level on
    /// purpose: the rule is about the *shape* of the API, and a runtime test
    /// cannot observe a signature that should not exist.
    #[test]
    fn falsifier_a5_no_string_window_parameter() {
        for source in [include_str!("navigation.rs"), include_str!("input.rs")] {
            for (line, text) in source.lines().enumerate() {
                let signature = text.trim_start();
                if !signature.starts_with("pub fn") {
                    continue;
                }
                let mentions_window = signature.to_ascii_lowercase().contains("window");
                let string_parameter = signature.contains("&str")
                    || signature.contains("&'a str")
                    || signature.contains(": String")
                    || signature.contains("&String");
                assert!(
                    !(mentions_window && string_parameter),
                    "line {}: a window entry point takes a string ({})",
                    line + 1,
                    signature
                );
            }
        }
    }

    /// A5 rule 3 — the stack has no duplicates, keeps painting order, and a
    /// closed middle window does not disturb the rest.
    ///
    /// Violation caught: an `open` that re-pushes (duplicate), a `close` that
    /// pops the top instead of the named window, and a depth that counts
    /// closes instead of opens.
    #[test]
    fn falsifier_a5_stack_invariants() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        nav.open(WindowId::INVENTORY).expect("inventory");
        nav.open(WindowId::QUESTS).expect("quests");
        nav.open(WindowId::HUD).expect("hud");
        assert_eq!(nav.stack(), &[WindowId::INVENTORY, WindowId::QUESTS, WindowId::HUD]);
        assert_eq!(nav.depth(), 3);
        assert_eq!(nav.top(), Some(WindowId::HUD));

        assert_eq!(
            nav.open(WindowId::QUESTS),
            Err(UiCompatError::Navigation(RULE_OPEN_DUPLICATE))
        );
        assert_eq!(nav.depth(), 3);

        nav.close(WindowId::QUESTS).expect("close middle");
        assert_eq!(nav.stack(), &[WindowId::INVENTORY, WindowId::HUD]);
        assert_eq!(nav.depth(), 2);
        assert_eq!(nav.top(), Some(WindowId::HUD));

        nav.open(WindowId::QUESTS).expect("reopen");
        assert_eq!(nav.stack(), &[WindowId::INVENTORY, WindowId::HUD, WindowId::QUESTS]);
        assert_eq!(nav.top(), Some(WindowId::QUESTS));
        assert!(nav.title(WindowId::QUESTS).is_some());
    }

    /// A5 rule 1 — an id the registry never saw is refused.
    ///
    /// Violation caught: accepting any `WindowId` and letting the shell's
    /// `Unknown` error surface as a panic or a silent ignore.
    #[test]
    fn falsifier_a5_unregistered_window_is_refused() {
        let mut nav = Navigation::new(UiShell::new(WindowRegistry::new()));
        assert_eq!(
            nav.open(WindowId::new(65_000)),
            Err(UiCompatError::Navigation(RULE_WINDOW_TYPED))
        );
        assert_eq!(nav.depth(), 0);
        assert_eq!(
            Navigation::canonical().expect("canonical").depth(),
            0,
            "canonical shell starts closed"
        );
    }
}
