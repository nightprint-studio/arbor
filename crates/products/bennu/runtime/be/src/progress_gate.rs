//! Rate limit for "done of total" progress events — by whether anyone is looking.
//!
//! A walk over a project calls its progress callback once **per file**, tens of thousands of times,
//! while a bar needs about ten updates a second. In the background the cost is worse than waste: the
//! shell and its webview are power-throttled and this process is not, so every event queues unread
//! and the whole backlog is delivered at once on the way back — the freeze `arbor_be::focus`
//! describes. Unfocused, one every few seconds keeps the card honest for whenever you return.
//!
//! The last event (`done >= total`) is never dropped: a bar that stops at 19 997 of 20 000 looks
//! stuck.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The shortest gap between two events while a window is in front — about ten a second, which is
/// what a moving bar needs and more than a person can read.
const FOCUSED_INTERVAL: Duration = Duration::from_millis(100);

/// …and while the app is in the background, where nobody is reading and the frontend that receives
/// it is throttled. Not zero: the card must be roughly right the moment you come back, and the
/// frontend reads a live stream as "still working".
const UNFOCUSED_INTERVAL: Duration = Duration::from_secs(3);

/// One progress stream's limiter. Shared across the threads of a parallel walk.
#[derive(Default)]
pub(crate) struct ProgressGate {
    last: Mutex<Option<Instant>>,
}

impl ProgressGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether the event for `done` of `total` should be sent now.
    pub(crate) fn allow(&self, done: usize, total: usize) -> bool {
        self.allow_at(done, total, Instant::now(), arbor_be::prelude::app_focused())
    }

    fn allow_at(&self, done: usize, total: usize, now: Instant, focused: bool) -> bool {
        if done >= total {
            return true;
        }
        let interval = if focused { FOCUSED_INTERVAL } else { UNFOCUSED_INTERVAL };
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        if last.is_some_and(|prev| now.duration_since(prev) < interval) {
            return false;
        }
        *last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_event_is_sent_and_a_close_follower_is_not() {
        let gate = ProgressGate::new();
        let t0 = Instant::now();
        assert!(gate.allow_at(1, 100, t0, true));
        assert!(!gate.allow_at(2, 100, t0 + Duration::from_millis(10), true));
        assert!(gate.allow_at(3, 100, t0 + Duration::from_millis(120), true));
    }

    #[test]
    fn the_background_gap_is_much_longer() {
        let gate = ProgressGate::new();
        let t0 = Instant::now();
        assert!(gate.allow_at(1, 100, t0, false));
        assert!(!gate.allow_at(2, 100, t0 + Duration::from_secs(1), false));
        assert!(gate.allow_at(3, 100, t0 + Duration::from_secs(4), false));
    }

    #[test]
    fn the_last_event_always_goes_out() {
        let gate = ProgressGate::new();
        let t0 = Instant::now();
        assert!(gate.allow_at(1, 100, t0, false));
        assert!(gate.allow_at(100, 100, t0 + Duration::from_millis(1), false));
    }
}
