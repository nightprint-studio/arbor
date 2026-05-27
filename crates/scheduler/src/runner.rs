//! Per-schedule async loop. One tokio task per registered entry.
//!
//! The loop is deliberately dumb: it sleeps until the next fire time,
//! checks the gates (focus + custom), fires the action, repeats. All
//! mutability (cancel / disable / trigger swap) flows in through
//! [`EntryShared`] — atomics for the flags, a single short-lived mutex
//! for the trigger state, and a [`tokio::sync::Notify`] to wake the loop
//! early when something changes.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use arbor_core::prelude::AppCtx;

use crate::scheduler::EntryShared;
use crate::trigger::NextWait;

pub(crate) async fn run(shared: Arc<EntryShared>, ctx: Arc<dyn AppCtx>) {
    // `fire_on_load` runs FIRST — semantics match "fire immediately at
    // load, then start the cadence" (i.e. before any `initial_delay`).
    if shared.opts.fire_on_load
        && shared.enabled.load(Ordering::Acquire)
        && !shared.cancelled.load(Ordering::Acquire)
    {
        shared.action.fire().await;
    }

    // Initial delay applies to FixedRate / FixedDelay only — for cron the
    // first fire is anchored to the next wall-clock match, so a delay
    // would just push past one occurrence.
    let initial = shared.opts.initial_delay;
    if !initial.is_zero() {
        let is_cron = shared.trigger.lock()
            .expect("trigger state poisoned")
            .compiled.is_cron();
        if !is_cron && !sleep_or_signal(initial, &shared).await {
            return;
        }
    }

    let mut last_start: Option<Instant> = None;

    loop {
        if shared.cancelled.load(Ordering::Acquire) { return; }

        // Park while disabled. Re-check after subscribing as a waiter so
        // a flip that happened just before the await isn't missed.
        if !shared.enabled.load(Ordering::Acquire) {
            let notified = shared.wake.notified();
            tokio::pin!(notified);
            if !shared.enabled.load(Ordering::Acquire)
                && !shared.cancelled.load(Ordering::Acquire)
            {
                notified.await;
            }
            continue;
        }

        let wait = {
            let g = shared.trigger.lock().expect("trigger state poisoned");
            g.compiled.next_wait(last_start)
        };
        let dur = match wait {
            NextWait::Sleep(d) => d,
            NextWait::Done => {
                tracing::info!(
                    schedule = %shared.key,
                    "cron schedule has no future occurrences — runner exiting"
                );
                return;
            }
        };

        if !dur.is_zero() && !sleep_or_signal(dur, &shared).await {
            return;
        }

        // Re-validate after the sleep — a trigger swap or disable may have
        // happened while we were waiting.
        if shared.cancelled.load(Ordering::Acquire) { return; }
        if !shared.enabled.load(Ordering::Acquire) { continue; }

        // Gating. Both "focus" and the custom gate treat "skip" as
        // "advance the clock": last_start is bumped so FixedRate doesn't
        // catch up with a burst when the gate re-opens.
        if shared.opts.only_when_focused && !ctx.is_focused() {
            last_start = Some(Instant::now());
            continue;
        }
        if let Some(gate) = &shared.opts.gate {
            if !gate() {
                last_start = Some(Instant::now());
                continue;
            }
        }

        last_start = Some(Instant::now());
        shared.action.fire().await;
    }
}

/// Sleep for `dur`, or wake early when [`EntryShared::wake`] is notified.
/// Returns `false` if the schedule was cancelled while we were waiting.
async fn sleep_or_signal(dur: Duration, shared: &Arc<EntryShared>) -> bool {
    let notified = shared.wake.notified();
    tokio::pin!(notified);
    tokio::select! {
        _ = tokio::time::sleep(dur) => {}
        _ = &mut notified           => {}
    }
    !shared.cancelled.load(Ordering::Acquire)
}
