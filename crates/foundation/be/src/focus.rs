//! Whether any of the app's windows currently has the OS focus — pushed down from the shell.
//!
//! ## Why a backend cares
//!
//! It is the producer in a producer/consumer pair whose halves are throttled differently. On
//! Windows the shell puts itself and its WebView2 processes into EcoQoS + `IDLE_PRIORITY_CLASS`
//! when the app goes to the background; a product backend is deliberately left at normal priority,
//! because the build it is running for you must not be demoted just because you alt-tabbed away.
//!
//! So the consumer slows down by two orders of magnitude and the producer does not. Every event
//! emitted meanwhile sits in the webview's queue, and the whole backlog is delivered at once when
//! the window comes back — thousands of handlers, each waking the frontend's reactivity, each
//! possibly asking the backend something in reply. That is the "the editor freezes for a few
//! seconds every time I come back to it" report, and no amount of coalescing on the frontend fixes
//! the part that has already been paid for: producing, framing and writing events nobody could
//! read.
//!
//! The frontend coalesces what it receives. This is the other half — the emitter's chance to not
//! send it in the first place.
//!
//! ## What it does NOT mean
//!
//! Not a licence to stop working, and not a licence to drop anything a person will want to read.
//! A build's log lines, a test's verdicts and a run's output are all still emitted in full: they
//! are the record of something that happened, and "you were looking elsewhere" is no reason to
//! lose them. What this gates is **progress**: a position that the next event supersedes, where
//! the only cost of skipping one is that a bar moves in bigger steps.
//!
//! Defaults to focused, so a backend that is never told behaves exactly as it did before.

use std::sync::atomic::{AtomicBool, Ordering};

/// The reserved method the shell calls to push the state down. Double-underscored like
/// [`crate::dispatch::TOOLS_METHOD`]: host plumbing, not product surface. Registered for every
/// backend by [`crate::dispatch::Dispatcher::new`], so none can forget it.
pub const FOCUS_METHOD: &str = "__app_focus";

static APP_FOCUSED: AtomicBool = AtomicBool::new(true);

/// True while at least one of the app's windows has the OS focus.
pub fn app_focused() -> bool {
    APP_FOCUSED.load(Ordering::Relaxed)
}

/// Record the shell's latest word on it. Called by the [`FOCUS_METHOD`] handler; public so a
/// backend can set it in a test.
pub fn set_app_focused(focused: bool) {
    APP_FOCUSED.store(focused, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not two, and deliberately: the state is a process-wide static, so two tests
    /// writing it would race each other inside the same test binary.
    #[test]
    fn defaults_to_focused_and_round_trips() {
        // Read before writing — this is the only test that touches the static, so what it reads
        // here is still the declared default. That the default is `true` is the contract: a
        // backend nobody ever tells behaves exactly as it did before any of this existed.
        assert!(app_focused(), "the default must be focused");
        set_app_focused(false);
        assert!(!app_focused());
        set_app_focused(true);
        assert!(app_focused());
    }
}
