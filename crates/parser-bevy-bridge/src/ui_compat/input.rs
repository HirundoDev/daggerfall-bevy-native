//! A6 — input axis: retail events, a total key map and a focus trail
//! (plan §3.6).
//!
//! **The rule:** one retail event reaches **at most one** window. The router
//! reads the top of the [`Navigation`] stack, records the delivery in an
//! ordered trail (`window`, `intent`, `order`) and returns either
//! [`Routed::Delivered`] or a typed [`Routed::Dropped`] reason — never both,
//! never two windows.
//!
//! `Cancel` is the one intent that mutates navigation: it closes the top
//! window and focus falls to the previous one; on a stack with a single root
//! window it is **dropped** (`"root-window"`), because a root window is not a
//! dialog and closing it would empty the screen. `Scroll` saturates the
//! selection inside `0..rows-1` instead of wrapping: a wrap would make
//! "scroll up at the top" indistinguishable from "scroll down".
//!
//! The key map is total: `map_key` is an exhaustive `match`, so adding a retail
//! key without deciding its intent is a compile error, and the inert keys
//! (`Backspace`, `F1`..`F10`) are listed as data rather than forgotten.

use daggerfall_nervous::events::WindowId;

use super::error::{DROP_EMPTY_LIST, DROP_NO_FOCUS, DROP_ROOT_WINDOW};
use super::navigation::Navigation;

/// Focus movement request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Next item (list order).
    Next,
    /// Previous item (list order).
    Prev,
}

impl Direction {
    /// Every direction, in declaration order.
    pub fn all() -> [Direction; 6] {
        [
            Self::Up,
            Self::Down,
            Self::Left,
            Self::Right,
            Self::Next,
            Self::Prev,
        ]
    }

    /// `-1` for the decreasing directions, `+1` for the increasing ones.
    pub fn delta(self) -> i32 {
        match self {
            Self::Up | Self::Left | Self::Prev => -1,
            Self::Down | Self::Right | Self::Next => 1,
        }
    }

    /// Stable name (for reports).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Next => "next",
            Self::Prev => "prev",
        }
    }
}

/// What a retail event asks the UI to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputIntent {
    /// Move the focus.
    Focus(Direction),
    /// Accept on the focused window.
    Activate,
    /// Dismiss the focused window.
    Cancel,
    /// Scroll the focused list by a signed amount.
    Scroll(i16),
    /// Drag inside the focused window (canvas coordinates).
    Drag {
        /// Press position.
        from: (u16, u16),
        /// Release position.
        to: (u16, u16),
    },
}

impl InputIntent {
    /// Stable name (for reports and the trail).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus(_) => "focus",
            Self::Activate => "activate",
            Self::Cancel => "cancel",
            Self::Scroll(_) => "scroll",
            Self::Drag { .. } => "drag",
        }
    }
}

/// Retail key codes the UI reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetailKey {
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Confirm.
    Enter,
    /// Dismiss.
    Escape,
    /// Next.
    Tab,
    /// Previous.
    BackTab,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// First item.
    Home,
    /// Last item.
    End,
    /// Confirm (space).
    Space,
    /// Inert.
    Backspace,
    /// Inert.
    F1,
    /// Inert.
    F2,
    /// Inert.
    F3,
    /// Inert.
    F4,
    /// Inert.
    F5,
    /// Inert.
    F6,
    /// Inert.
    F7,
    /// Inert.
    F8,
    /// Inert.
    F9,
    /// Inert.
    F10,
}

impl RetailKey {
    /// Every key, in declaration order (the totality witness).
    pub fn all() -> [RetailKey; 24] {
        [
            Self::Up,
            Self::Down,
            Self::Left,
            Self::Right,
            Self::Enter,
            Self::Escape,
            Self::Tab,
            Self::BackTab,
            Self::PageUp,
            Self::PageDown,
            Self::Home,
            Self::End,
            Self::Space,
            Self::Backspace,
            Self::F1,
            Self::F2,
            Self::F3,
            Self::F4,
            Self::F5,
            Self::F6,
            Self::F7,
            Self::F8,
            Self::F9,
            Self::F10,
        ]
    }

    /// Keys the UI deliberately ignores.
    pub fn inert() -> [RetailKey; 11] {
        [
            Self::Backspace,
            Self::F1,
            Self::F2,
            Self::F3,
            Self::F4,
            Self::F5,
            Self::F6,
            Self::F7,
            Self::F8,
            Self::F9,
            Self::F10,
        ]
    }

    /// Stable name (for reports).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Enter => "enter",
            Self::Escape => "escape",
            Self::Tab => "tab",
            Self::BackTab => "backtab",
            Self::PageUp => "pageup",
            Self::PageDown => "pagedown",
            Self::Home => "home",
            Self::End => "end",
            Self::Space => "space",
            Self::Backspace => "backspace",
            Self::F1 => "f1",
            Self::F2 => "f2",
            Self::F3 => "f3",
            Self::F4 => "f4",
            Self::F5 => "f5",
            Self::F6 => "f6",
            Self::F7 => "f7",
            Self::F8 => "f8",
            Self::F9 => "f9",
            Self::F10 => "f10",
        }
    }
}

/// Mouse button the UI reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Accept.
    Left,
    /// Dismiss.
    Right,
}

/// A canvas-space mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    /// Canvas column.
    pub x: u16,
    /// Canvas row.
    pub y: u16,
    /// Button, `None` for a move.
    pub button: Option<MouseButton>,
}

/// The key map (total — an exhaustive match decides every retail key).
pub fn map_key(key: RetailKey) -> Option<InputIntent> {
    match key {
        RetailKey::Up => Some(InputIntent::Focus(Direction::Up)),
        RetailKey::Down => Some(InputIntent::Focus(Direction::Down)),
        RetailKey::Left => Some(InputIntent::Focus(Direction::Left)),
        RetailKey::Right => Some(InputIntent::Focus(Direction::Right)),
        RetailKey::Enter | RetailKey::Space => Some(InputIntent::Activate),
        RetailKey::Escape => Some(InputIntent::Cancel),
        RetailKey::Tab | RetailKey::PageDown => Some(InputIntent::Focus(Direction::Next)),
        RetailKey::BackTab | RetailKey::PageUp => Some(InputIntent::Focus(Direction::Prev)),
        RetailKey::Home => Some(InputIntent::Scroll(i16::MIN)),
        RetailKey::End => Some(InputIntent::Scroll(i16::MAX)),
        RetailKey::Backspace
        | RetailKey::F1
        | RetailKey::F2
        | RetailKey::F3
        | RetailKey::F4
        | RetailKey::F5
        | RetailKey::F6
        | RetailKey::F7
        | RetailKey::F8
        | RetailKey::F9
        | RetailKey::F10 => None,
    }
}

/// The mouse map: a left press activates, a right press cancels, a move does
/// nothing (the UI has no hover state by contract).
pub fn map_mouse(event: MouseEvent) -> Option<InputIntent> {
    match event.button {
        Some(MouseButton::Left) => Some(InputIntent::Activate),
        Some(MouseButton::Right) => Some(InputIntent::Cancel),
        None => None,
    }
}

/// A press/release pair is a drag only when the button is the left one and the
/// position actually changed.
pub fn map_drag(press: MouseEvent, release: MouseEvent) -> Option<InputIntent> {
    let left = press.button == Some(MouseButton::Left) || release.button == Some(MouseButton::Left);
    if !left || (press.x, press.y) == (release.x, release.y) {
        return None;
    }
    Some(InputIntent::Drag {
        from: (press.x, press.y),
        to: (release.x, release.y),
    })
}

/// Outcome of one dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routed {
    /// Delivered to exactly one window.
    Delivered {
        /// Window that received it.
        window: WindowId,
        /// Intent delivered.
        intent: InputIntent,
        /// 1-based delivery order (strictly increasing).
        order: u32,
    },
    /// Dropped, with a static reason.
    Dropped(&'static str),
}

impl Routed {
    /// Window that received the intent, if any.
    pub fn delivered_window(self) -> Option<WindowId> {
        match self {
            Self::Delivered { window, .. } => Some(window),
            Self::Dropped(_) => None,
        }
    }

    /// Drop reason, if dropped.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Delivered { .. } => None,
            Self::Dropped(reason) => Some(reason),
        }
    }

    /// One-line label for evidence.
    pub fn label(self) -> String {
        match self {
            Self::Delivered {
                window,
                intent,
                order,
            } => format!("#{order} {} -> {:?}", intent.as_str(), window),
            Self::Dropped(reason) => format!("dropped({reason})"),
        }
    }
}

/// Routes intents to the top of the navigation stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRouter {
    focus: Option<WindowId>,
    trail: Vec<(WindowId, InputIntent, u32)>,
    order: u32,
    rows: usize,
    selection: usize,
}

impl InputRouter {
    /// Router focused on the current top of `nav`.
    pub fn new(nav: &Navigation) -> Self {
        Self {
            focus: nav.top(),
            trail: Vec::new(),
            order: 0,
            rows: 0,
            selection: 0,
        }
    }

    /// Re-reads the focus from the stack (after navigation changed elsewhere).
    pub fn sync_focus(&mut self, nav: &Navigation) {
        self.focus = nav.top();
    }

    /// Current focus.
    pub fn focus(&self) -> Option<WindowId> {
        self.focus
    }

    /// Declares the focused list length; clamps the selection.
    pub fn set_list(&mut self, rows: usize) {
        self.rows = rows;
        self.selection = if rows == 0 {
            0
        } else {
            self.selection.min(rows - 1)
        };
    }

    /// Current selection, `None` when the list has no rows.
    pub fn selection(&self) -> Option<usize> {
        if self.rows == 0 {
            None
        } else {
            Some(self.selection)
        }
    }

    /// Delivery trail in order.
    pub fn trail(&self) -> &[(WindowId, InputIntent, u32)] {
        &self.trail
    }

    /// Routes one intent (plan §3.6 rules 1-5).
    ///
    /// `nav` is needed for the one intent that closes a window; the router
    /// never clones the stack and never holds a second copy of it.
    pub fn dispatch(&mut self, nav: &mut Navigation, intent: InputIntent) -> Routed {
        let Some(window) = nav.top() else {
            self.focus = None;
            return Routed::Dropped(DROP_NO_FOCUS);
        };
        self.focus = Some(window);

        match intent {
            InputIntent::Scroll(delta) => {
                if self.rows == 0 {
                    return Routed::Dropped(DROP_EMPTY_LIST);
                }
                let last = (self.rows - 1) as i64;
                let moved = self.selection as i64 + delta as i64;
                self.selection = moved.clamp(0, last) as usize;
            }
            InputIntent::Focus(direction) => {
                if self.rows > 0 {
                    let last = (self.rows - 1) as i64;
                    let moved = self.selection as i64 + direction.delta() as i64;
                    self.selection = moved.clamp(0, last) as usize;
                }
            }
            InputIntent::Cancel => {
                if nav.depth() <= 1 {
                    return Routed::Dropped(DROP_ROOT_WINDOW);
                }
                if nav.close(window).is_err() {
                    return Routed::Dropped(DROP_NO_FOCUS);
                }
                self.focus = nav.top();
            }
            InputIntent::Activate | InputIntent::Drag { .. } => {}
        }

        self.order += 1;
        self.trail.push((window, intent, self.order));
        Routed::Delivered {
            window,
            intent,
            order: self.order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack() -> (Navigation, InputRouter) {
        let mut nav = Navigation::canonical().expect("canonical shell");
        nav.open(WindowId::HUD).expect("hud");
        let router = InputRouter::new(&nav);
        (nav, router)
    }

    /// A6 rule 1 — with nothing open there is no delivery at all.
    ///
    /// Violation caught: a fallback focus (the HUD, "the first window") that
    /// swallows input when the stack is empty.
    #[test]
    fn falsifier_a6_empty_stack_drops() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        let mut router = InputRouter::new(&nav);
        assert_eq!(router.focus(), None);
        assert_eq!(
            router.dispatch(&mut nav, InputIntent::Activate),
            Routed::Dropped(DROP_NO_FOCUS)
        );
        assert!(router.trail().is_empty());
        assert_eq!(router.selection(), None);
    }

    /// A6 rule 1 — one event, one window, one trail row.
    ///
    /// Violation caught: fan-out delivery (the same event reaching the window
    /// below), a trail row recorded for a dropped event, and an order that
    /// repeats.
    #[test]
    fn falsifier_a6_one_event_one_window() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        nav.open(WindowId::HUD).expect("hud");
        nav.open(WindowId::INVENTORY).expect("inventory");
        let mut router = InputRouter::new(&nav);
        assert_eq!(router.focus(), Some(WindowId::INVENTORY));

        let first = router.dispatch(&mut nav, InputIntent::Activate);
        let second = router.dispatch(&mut nav, InputIntent::Focus(Direction::Down));
        let third = router.dispatch(&mut nav, InputIntent::Drag { from: (1, 1), to: (4, 4) });
        for (routed, order) in [(first, 1u32), (second, 2), (third, 3)] {
            assert_eq!(
                routed,
                Routed::Delivered {
                    window: WindowId::INVENTORY,
                    intent: match order {
                        1 => InputIntent::Activate,
                        2 => InputIntent::Focus(Direction::Down),
                        _ => InputIntent::Drag { from: (1, 1), to: (4, 4) },
                    },
                    order,
                }
            );
            assert_eq!(routed.delivered_window(), Some(WindowId::INVENTORY));
            assert_eq!(routed.reason(), None);
        }
        assert_eq!(router.trail().len(), 3);
        let orders: Vec<u32> = router.trail().iter().map(|row| row.2).collect();
        assert_eq!(orders, vec![1, 2, 3]);
        assert!(router.trail().iter().all(|row| row.0 == WindowId::INVENTORY));
    }

    /// A6 rule 3 — `Cancel` closes the top window and focus falls back; on the
    /// root window it is dropped.
    ///
    /// Violation caught: `Cancel` closing the *root* (emptying the screen),
    /// `Cancel` leaving the focus on the closed window, and `Cancel` being
    /// dropped on a dialog stack.
    #[test]
    fn falsifier_a6_cancel_closes_top() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        nav.open(WindowId::HUD).expect("hud");
        nav.open(WindowId::INVENTORY).expect("inventory");
        nav.open(WindowId::QUESTS).expect("quests");
        let mut router = InputRouter::new(&nav);

        let routed = router.dispatch(&mut nav, InputIntent::Cancel);
        assert_eq!(
            routed,
            Routed::Delivered {
                window: WindowId::QUESTS,
                intent: InputIntent::Cancel,
                order: 1,
            }
        );
        assert!(!nav.is_open(WindowId::QUESTS));
        assert_eq!(nav.top(), Some(WindowId::INVENTORY));
        assert_eq!(router.focus(), Some(WindowId::INVENTORY));

        router.dispatch(&mut nav, InputIntent::Cancel);
        assert_eq!(nav.depth(), 1);
        assert_eq!(
            router.dispatch(&mut nav, InputIntent::Cancel),
            Routed::Dropped(DROP_ROOT_WINDOW)
        );
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.top(), Some(WindowId::HUD));
        assert_eq!(router.trail().len(), 2);
    }

    /// A6 rule 4 — `Scroll` saturates, never wraps, and needs rows.
    ///
    /// Violation caught: a wrapping selection (`0 - 1` becoming `n-1`), an
    /// out-of-range selection after a large delta, and a scroll on an empty
    /// list being delivered as if it moved something.
    #[test]
    fn falsifier_a6_scroll_saturates() {
        let (mut nav, mut router) = stack();
        router.set_list(5);
        assert_eq!(router.selection(), Some(0));
        router.dispatch(&mut nav, InputIntent::Scroll(-1));
        assert_eq!(router.selection(), Some(0));
        router.dispatch(&mut nav, InputIntent::Scroll(10));
        assert_eq!(router.selection(), Some(4));
        router.dispatch(&mut nav, InputIntent::Scroll(i16::MAX));
        assert_eq!(router.selection(), Some(4));
        router.dispatch(&mut nav, InputIntent::Scroll(i16::MIN));
        assert_eq!(router.selection(), Some(0));
        router.dispatch(&mut nav, InputIntent::Focus(Direction::Next));
        assert_eq!(router.selection(), Some(1));
        router.dispatch(&mut nav, InputIntent::Focus(Direction::Prev));
        assert_eq!(router.selection(), Some(0));

        router.set_list(0);
        assert_eq!(router.selection(), None);
        assert_eq!(
            router.dispatch(&mut nav, InputIntent::Scroll(1)),
            Routed::Dropped(DROP_EMPTY_LIST)
        );
        router.set_list(2);
        assert_eq!(router.selection(), Some(0));
    }

    /// A6 rule 5 — the key map is total and the inert set is declared.
    ///
    /// Violation caught: a retail key with no decision (a `_ => None` arm), a
    /// direction with no key, and a dispatch that panics on any intent.
    #[test]
    fn falsifier_a6_key_map_is_total() {
        let inert = RetailKey::inert();
        for key in RetailKey::all() {
            let mapped = map_key(key);
            if inert.contains(&key) {
                assert_eq!(mapped, None, "{} must be inert", key.as_str());
            } else {
                assert!(mapped.is_some(), "{} has no mapping", key.as_str());
            }
        }
        assert_eq!(map_key(RetailKey::Up), Some(InputIntent::Focus(Direction::Up)));
        assert_eq!(map_key(RetailKey::Tab), Some(InputIntent::Focus(Direction::Next)));
        assert_eq!(map_key(RetailKey::Escape), Some(InputIntent::Cancel));
        assert_eq!(map_key(RetailKey::Home), Some(InputIntent::Scroll(i16::MIN)));
        assert_eq!(
            map_key(RetailKey::Enter),
            Some(InputIntent::Activate)
        );

        for direction in Direction::all() {
            assert!(
                RetailKey::all()
                    .into_iter()
                    .any(|key| map_key(key) == Some(InputIntent::Focus(direction))),
                "direction {} has no key",
                direction.as_str()
            );
        }
        assert_eq!(Direction::all().map(Direction::delta), [-1, 1, -1, 1, 1, -1]);

        let left = MouseEvent {
            x: 3,
            y: 4,
            button: Some(MouseButton::Left),
        };
        let right = MouseEvent {
            x: 3,
            y: 4,
            button: Some(MouseButton::Right),
        };
        let move_only = MouseEvent {
            x: 9,
            y: 9,
            button: None,
        };
        assert_eq!(map_mouse(left), Some(InputIntent::Activate));
        assert_eq!(map_mouse(right), Some(InputIntent::Cancel));
        assert_eq!(map_mouse(move_only), None);
        assert_eq!(
            map_drag(left, MouseEvent { x: 8, y: 4, button: None }),
            Some(InputIntent::Drag {
                from: (3, 4),
                to: (8, 4)
            })
        );
        assert_eq!(map_drag(left, left), None);

        let (mut nav, mut router) = stack();
        router.set_list(3);
        for intent in [
            InputIntent::Focus(Direction::Up),
            InputIntent::Activate,
            InputIntent::Drag { from: (0, 0), to: (1, 1) },
            InputIntent::Scroll(1),
        ] {
            assert!(matches!(
                router.dispatch(&mut nav, intent),
                Routed::Delivered { .. }
            ));
        }
        assert_eq!(router.trail().len(), 4);
    }
}
