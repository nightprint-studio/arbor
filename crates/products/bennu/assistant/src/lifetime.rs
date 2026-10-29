//! How long something the assistant started may outlive the run that started it.
//!
//! A debug session belongs to the **conversation**, not to the message that opened it: the program stays
//! alive between messages so the next one can pick up where the last stopped. What bounds it is a [`Lease`]:
//! it expires after a stretch with no use of any debug tool of that conversation. The clock is injected
//! (plain milliseconds), so the rule is tested without waiting.

pub const DEFAULT_IDLE_MINUTES: u32 = 30;
pub const MIN_IDLE_MINUTES: u32 = 5;
pub const MAX_IDLE_MINUTES: u32 = 240;

/// The configured idle minutes, kept to a sane range; `0` (unset) is the default.
pub fn clamp_idle_minutes(minutes: u32) -> u32 {
    if minutes == 0 { DEFAULT_IDLE_MINUTES } else { minutes.clamp(MIN_IDLE_MINUTES, MAX_IDLE_MINUTES) }
}

/// A right to stay alive that every use renews.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lease {
    last_use_ms: u64,
    idle_ms: u64,
}

impl Lease {
    pub fn new(now_ms: u64, idle_minutes: u32) -> Self {
        Lease { last_use_ms: now_ms, idle_ms: u64::from(clamp_idle_minutes(idle_minutes)) * 60_000 }
    }

    /// A use of the thing the lease covers (any debug tool, or a stop).
    pub fn touch(&mut self, now_ms: u64) {
        self.last_use_ms = self.last_use_ms.max(now_ms);
    }

    /// Change the idle allowance (the setting moved); the time already idle still counts.
    pub fn set_idle_minutes(&mut self, idle_minutes: u32) {
        self.idle_ms = u64::from(clamp_idle_minutes(idle_minutes)) * 60_000;
    }

    pub fn expired(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_use_ms) >= self.idle_ms
    }

    /// Whole minutes left, rounded up, for the "ends in N min" the panel shows; `0` once expired.
    pub fn remaining_minutes(&self, now_ms: u64) -> u64 {
        let left = self.idle_ms.saturating_sub(now_ms.saturating_sub(self.last_use_ms));
        left.div_ceil(60_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    #[test]
    fn the_idle_setting_is_kept_to_a_sane_range() {
        assert_eq!(clamp_idle_minutes(0), DEFAULT_IDLE_MINUTES);
        assert_eq!(clamp_idle_minutes(1), MIN_IDLE_MINUTES);
        assert_eq!(clamp_idle_minutes(60), 60);
        assert_eq!(clamp_idle_minutes(100_000), MAX_IDLE_MINUTES);
    }

    #[test]
    fn a_lease_expires_after_the_idle_time_and_not_before() {
        let lease = Lease::new(1_000, 30);
        assert!(!lease.expired(1_000 + 29 * MIN));
        assert!(lease.expired(1_000 + 30 * MIN));
    }

    #[test]
    fn using_it_renews_it() {
        let mut lease = Lease::new(0, 30);
        lease.touch(20 * MIN);
        assert!(!lease.expired(49 * MIN));
        assert!(lease.expired(50 * MIN));
    }

    #[test]
    fn an_older_touch_never_shortens_the_lease() {
        let mut lease = Lease::new(100 * MIN, 30);
        lease.touch(10 * MIN);
        assert!(!lease.expired(120 * MIN));
    }

    #[test]
    fn the_panel_is_told_whole_minutes_rounded_up() {
        let lease = Lease::new(0, 30);
        assert_eq!(lease.remaining_minutes(0), 30);
        assert_eq!(lease.remaining_minutes(MIN + 1), 29);
        assert_eq!(lease.remaining_minutes(30 * MIN), 0);
        assert_eq!(lease.remaining_minutes(99 * MIN), 0);
    }

    #[test]
    fn changing_the_allowance_keeps_the_time_already_idle() {
        let mut lease = Lease::new(0, 60);
        lease.set_idle_minutes(10);
        assert!(lease.expired(15 * MIN));
    }
}
