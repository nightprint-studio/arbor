//! When to send a failed request again, and how long to wait first.
//!
//! The rules are separate from the loop so they can be tested with numbers instead of a clock. A request is
//! retried only for a failure that can pass by itself (see [`ErrorKind::retryable`]): the wait grows
//! exponentially from a second to half a minute, carries jitter (a parent and its sub-agents that fail
//! together must not retry together) and never undercuts a `retry-after` the provider sent. Two budgets keep
//! it from going on forever: a **network** that is down is waited out for a few minutes in total, and a
//! **provider** that keeps answering "not now" gets a handful of attempts.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::{Duration, Instant};

use crate::error::ProviderError;

/// How long a network that is down is waited out, in total, before giving up.
pub const NETWORK_BUDGET: Duration = Duration::from_secs(180);
/// How many times a provider that answers 429, 529 or 5xx is asked again.
pub const SERVER_ATTEMPTS: u32 = 5;
/// The first wait; each failure doubles it.
pub const BASE_WAIT: Duration = Duration::from_secs(1);
/// No single wait is longer than this, however many failures there have been.
pub const MAX_WAIT: Duration = Duration::from_secs(30);
/// A `retry-after` longer than this is not waited for in silence: the failure is reported instead.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// The numbers behind the rules; constants for a run, smaller for a helper job the user is not watching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub base: Duration,
    pub cap: Duration,
    pub network_budget: Duration,
    pub server_attempts: u32,
    pub max_retry_after: Duration,
}

impl RetryPolicy {
    /// For a run: the user is looking at it and can stop it.
    pub const fn standard() -> Self {
        Self { base: BASE_WAIT, cap: MAX_WAIT, network_budget: NETWORK_BUDGET, server_attempts: SERVER_ATTEMPTS, max_retry_after: MAX_RETRY_AFTER }
    }

    /// For compaction and other jobs that block something while they wait: a short grace, then the failure.
    pub const fn quick() -> Self {
        Self { base: BASE_WAIT, cap: Duration::from_secs(8), network_budget: Duration::from_secs(20), server_attempts: 2, max_retry_after: Duration::from_secs(20) }
    }

    /// About how many retries the network budget allows (for the "attempt 3/11" of the status line); the
    /// real number depends on the jitter, so this is the count at its mean.
    pub fn planned_network_attempts(&self) -> u32 {
        let mut waited = Duration::ZERO;
        let mut n = 0;
        loop {
            let next = self.jittered(n + 1, 0.5);
            if waited + next > self.network_budget || n >= 64 {
                return n.max(1);
            }
            waited += next;
            n += 1;
        }
    }

    /// The wait after the `failures`-th failure in a row: doubled each time up to the cap, then spread by
    /// `jitter` in `[0, 1)` between half and all of it.
    fn jittered(&self, failures: u32, jitter: f64) -> Duration {
        let doubled = self.base.saturating_mul(1u32 << failures.saturating_sub(1).min(16));
        let capped = doubled.min(self.cap);
        capped.mul_f64(0.5 + 0.5 * jitter.clamp(0.0, 1.0))
    }
}

/// One scheduled retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub wait: Duration,
    /// Which retry this is (1 for the first).
    pub attempt: u32,
    /// About how many there will be at most.
    pub max: u32,
}

/// The retries of one request: counts what was spent so far and decides on the next.
#[derive(Debug, Clone)]
pub struct Backoff {
    policy: RetryPolicy,
    failures: u32,
    server_failures: u32,
    waited: Duration,
}

impl Backoff {
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy, failures: 0, server_failures: 0, waited: Duration::ZERO }
    }

    /// After `error`: how long to wait before asking again, or `None` to give up (it cannot pass by itself,
    /// or the budget for it is spent). `jitter` is in `[0, 1)`.
    pub fn next(&mut self, error: &ProviderError, jitter: f64) -> Option<Plan> {
        if !error.retryable() {
            return None;
        }
        let failures = self.failures + 1;
        let mut wait = self.policy.jittered(failures, jitter);
        if let Some(asked) = error.retry_after {
            if asked > self.policy.max_retry_after {
                return None;
            }
            // What the provider asked is a floor; a second of jitter keeps clients that were told the same from coming back together.
            wait = wait.max(asked + Duration::from_secs_f64(jitter.clamp(0.0, 1.0)));
        }
        let max = if error.kind.is_network() {
            if self.waited + wait > self.policy.network_budget {
                return None;
            }
            self.waited += wait;
            self.policy.planned_network_attempts()
        } else {
            if self.server_failures >= self.policy.server_attempts {
                return None;
            }
            self.server_failures += 1;
            self.policy.server_attempts
        };
        self.failures = failures;
        Some(Plan { wait, attempt: failures, max: max.max(failures) })
    }
}

/// How a wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    /// The time passed.
    Elapsed,
    /// The user stopped the run: the wait is over at once.
    Cancelled,
    /// The user asked to try now.
    Now,
}

/// Sleep for `wait` in short slices, so a stop or a "retry now" is felt within a tenth of a second.
pub fn sleep_interruptible(wait: Duration, cancelled: &dyn Fn() -> bool, skip: &dyn Fn() -> bool) -> Pause {
    let until = Instant::now() + wait;
    loop {
        if cancelled() {
            return Pause::Cancelled;
        }
        if skip() {
            return Pause::Now;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Pause::Elapsed;
        }
        std::thread::sleep(left.min(Duration::from_millis(100)));
    }
}

/// A number in `[0, 1)` that differs from one call (and one process) to the next. `RandomState` is seeded
/// from the operating system, which is all the randomness a retry delay needs.
pub fn entropy_jitter() -> f64 {
    let bits = RandomState::new().build_hasher().finish() >> 11;
    bits as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn offline() -> ProviderError {
        ProviderError::offline("dns error")
    }

    fn waits(error: &ProviderError, jitter: f64, policy: RetryPolicy) -> Vec<u64> {
        let mut b = Backoff::new(policy);
        std::iter::from_fn(|| b.next(error, jitter).map(|p| p.wait.as_secs_f64().round() as u64)).take(100).collect()
    }

    #[test]
    fn the_wait_doubles_up_to_the_cap() {
        // jitter 1.0 = the full delay.
        let w = waits(&offline(), 1.0, RetryPolicy::standard());
        assert_eq!(&w[..6], &[1, 2, 4, 8, 16, 30]);
        assert!(w[6..].iter().all(|s| *s == 30));
    }

    #[test]
    fn jitter_moves_each_wait_between_half_and_all_of_it() {
        let low = waits(&offline(), 0.0, RetryPolicy::standard());
        let high = waits(&offline(), 0.999, RetryPolicy::standard());
        assert_eq!(low[2], 2); // 4 s halved
        assert_eq!(high[2], 4);
        assert_ne!(Backoff::new(RetryPolicy::standard()).next(&offline(), 0.1), Backoff::new(RetryPolicy::standard()).next(&offline(), 0.9), "two clients do not share a schedule");
    }

    #[test]
    fn a_network_that_stays_down_is_given_up_on_after_the_budget() {
        let policy = RetryPolicy::standard();
        let w = waits(&offline(), 1.0, policy);
        let total: u64 = w.iter().sum();
        assert!(total <= policy.network_budget.as_secs(), "{total}");
        assert!(total + 30 > policy.network_budget.as_secs(), "it used the budget: {total}");
        assert!(w.len() >= 8);
    }

    #[test]
    fn a_provider_that_says_not_now_gets_five_attempts() {
        for kind in [ErrorKind::Overloaded, ErrorKind::Server, ErrorKind::RateLimited] {
            let w = waits(&ProviderError::new(kind, "x"), 1.0, RetryPolicy::standard());
            assert_eq!(w.len(), SERVER_ATTEMPTS as usize, "{kind:?}");
        }
    }

    #[test]
    fn retry_after_is_a_floor() {
        let e = ProviderError::new(ErrorKind::RateLimited, "x").with_retry_after(Some(Duration::from_secs(20)));
        let plan = Backoff::new(RetryPolicy::standard()).next(&e, 0.0).unwrap();
        assert!(plan.wait >= Duration::from_secs(20), "{:?}", plan.wait);
        assert!(plan.wait < Duration::from_secs(22));
    }

    #[test]
    fn a_retry_after_longer_than_we_wait_in_silence_ends_the_retries() {
        let e = ProviderError::new(ErrorKind::RateLimited, "x").with_retry_after(Some(Duration::from_secs(600)));
        assert_eq!(Backoff::new(RetryPolicy::standard()).next(&e, 0.5), None);
    }

    #[test]
    fn what_cannot_pass_by_itself_is_never_retried() {
        for kind in [ErrorKind::Auth, ErrorKind::Billing, ErrorKind::ContextTooLong, ErrorKind::ModelNotFound, ErrorKind::BadRequest, ErrorKind::Cancelled, ErrorKind::Local] {
            assert_eq!(Backoff::new(RetryPolicy::standard()).next(&ProviderError::new(kind, "x"), 0.5), None, "{kind:?}");
        }
    }

    #[test]
    fn the_plan_counts_attempts_and_never_says_more_than_the_maximum() {
        let mut b = Backoff::new(RetryPolicy::standard());
        let first = b.next(&ProviderError::new(ErrorKind::Overloaded, "x"), 0.5).unwrap();
        let second = b.next(&ProviderError::new(ErrorKind::Overloaded, "x"), 0.5).unwrap();
        assert_eq!((first.attempt, second.attempt), (1, 2));
        assert_eq!(first.max, SERVER_ATTEMPTS);
        let mut n = Backoff::new(RetryPolicy::standard());
        while let Some(p) = n.next(&offline(), 0.5) {
            assert!(p.attempt <= p.max, "{p:?}");
        }
    }

    #[test]
    fn the_quick_policy_gives_up_sooner() {
        let quick: u64 = waits(&offline(), 1.0, RetryPolicy::quick()).iter().sum();
        let std_: u64 = waits(&offline(), 1.0, RetryPolicy::standard()).iter().sum();
        assert!(quick <= 20 && quick < std_);
    }

    #[test]
    fn a_wait_ends_at_once_when_the_run_is_stopped_or_told_to_go() {
        let started = Instant::now();
        assert_eq!(sleep_interruptible(Duration::from_secs(30), &|| true, &|| false), Pause::Cancelled);
        assert_eq!(sleep_interruptible(Duration::from_secs(30), &|| false, &|| true), Pause::Now);
        assert_eq!(sleep_interruptible(Duration::from_millis(30), &|| false, &|| false), Pause::Elapsed);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn jitter_stays_in_the_unit_interval() {
        for _ in 0..200 {
            let j = entropy_jitter();
            assert!((0.0..1.0).contains(&j));
        }
    }
}
