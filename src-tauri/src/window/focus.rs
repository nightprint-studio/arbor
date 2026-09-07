//! Whether **the app** has the OS focus — as opposed to any one of its windows.
//!
//! `WindowEvent::Focused` is per window, and everything downstream of it is per process: EcoQoS is
//! set on this process and on the WebView2 tree that every window shares, `AppState::app_focused`
//! is one flag, and a backend serves whichever windows exist. Feeding a per-window event straight
//! into those meant the two events of an ordinary window switch — `Focused(false)` for the one you
//! left, `Focused(true)` for the one you arrived at — fought each other, and if the blur landed
//! last (their order is not guaranteed, and the throttle controller debounces 150ms before acting
//! on whatever it sees) the whole app spent the session in efficiency mode **while you were using
//! it**: every window, because they share the processes.
//!
//! So the focused labels are tracked as a set, and what the rest of the app is told is whether the
//! set is empty. Moving between two Arbor windows never reports a loss of focus, because there was
//! none.
//!
//! Labels are removed on `Destroyed` as well as on blur: a window closed while focused would
//! otherwise stay in the set for ever and the app would never throttle again.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// The labels of the windows that currently have the focus, and the transition logic over them.
/// A plain value with no globals in it, so it can be exercised directly.
#[derive(Default)]
struct FocusSet(HashSet<String>);

impl FocusSet {
    /// Record `label`'s focus state, returning the app-wide answer **if it just changed** and
    /// `None` if it did not.
    ///
    /// `None` is the common case of a window switch, and returning it is the point: the caller
    /// acts only on a real transition, so nothing is scheduled for a change that is not one.
    fn set(&mut self, label: &str, is_focused: bool) -> Option<bool> {
        let was_any = !self.0.is_empty();
        if is_focused {
            self.0.insert(label.to_string());
        } else {
            self.0.remove(label);
        }
        let now_any = !self.0.is_empty();
        (now_any != was_any).then_some(now_any)
    }
}

fn focus_set() -> &'static Mutex<FocusSet> {
    static FOCUSED: OnceLock<Mutex<FocusSet>> = OnceLock::new();
    FOCUSED.get_or_init(|| Mutex::new(FocusSet::default()))
}

/// Record `label`'s focus state process-wide. See [`FocusSet::set`] for the return contract.
pub fn set_window_focused(label: &str, is_focused: bool) -> Option<bool> {
    focus_set()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set(label, is_focused)
}

/// A window is gone — forget it. Same contract as [`set_window_focused`]: `Some(false)` when
/// closing it is what took the app out of focus.
pub fn forget_window(label: &str) -> Option<bool> {
    set_window_focused(label, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this module exists for: switching between two Arbor windows must not report the
    /// app losing focus, in either delivery order.
    #[test]
    fn a_window_switch_is_not_a_focus_change() {
        let mut set = FocusSet::default();
        assert_eq!(set.set("a", true), Some(true));

        // Arrive-then-leave (the order Windows usually delivers).
        assert_eq!(set.set("b", true), None, "gaining a second window is not a change");
        assert_eq!(set.set("a", false), None, "leaving the first is not a change");

        // …and leave-then-arrive, which is the order that used to leave the app throttled while
        // the window you had just switched to was in front of you.
        assert_eq!(set.set("b", false), Some(false));
        assert_eq!(set.set("a", true), Some(true));
        assert_eq!(set.set("b", true), None);
        assert_eq!(set.set("a", false), None);

        assert_eq!(set.set("b", false), Some(false), "the last one out turns the light off");
    }

    /// A window closed while focused must not hold the app "focused" for ever, and the ordinary
    /// close sequence (blur, then destroy) must not report the change twice.
    #[test]
    fn forgetting_a_window_releases_the_app_and_is_idempotent() {
        let mut set = FocusSet::default();
        assert_eq!(set.set("solo", true), Some(true));
        assert_eq!(set.set("solo", false), Some(false));
        assert_eq!(set.set("solo", false), None);
    }
}
