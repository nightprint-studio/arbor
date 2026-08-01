//! The background sync probe — the only thing in Garrulus that talks to the
//! remote without a click, and it is read-only by construction.
//!
//! `docs/garrulus-design.md` §4.2 is the whole rationale and it is worth stating
//! where the code is: **everything that changes bytes happens because the user
//! pressed the title-bar button.** A background commit, pull or push would not be
//! a feature with a rough edge, it would be the bug — a note the user was still
//! thinking about, published to the other machine, or a remote change landing
//! under their cursor mid-sentence. So the tick's entire body is
//! `SyncRemote::probe` — the one method of that trait which cannot write — and
//! the result is an event.
//!
//! ## Why the scheduler and not a thread
//!
//! `arbor-scheduler` already owns cadence, the per-tick gate, cancel-on-notify and
//! reschedule-without-teardown. The watcher next door runs its own OS thread
//! because `notify`'s callback is synchronous and it has to own the watcher
//! handle; this has neither excuse.
//!
//! ## Two gotchas that shape this file
//!
//! 1. **`ScheduleOpts::only_when_focused` is useless in a headless backend.**
//!    `BackendAppCtx::is_focused()` is hardcoded `false` (there is no window to
//!    ask), so the scheduler's own focus gate would disable the probe forever.
//!    The real signal is the flag the frontend pushes through
//!    [`garrulus_set_focus`], read by a custom gate.
//! 2. **The tick is awaited directly, and holds nothing while it waits.** It
//!    clones the remote out of the state (`GarrulusState::remote`) rather than
//!    borrowing it under a guard, and `SyncRemote` implementations put their
//!    blocking work — git, the filesystem, and the credential `host_call` a fetch
//!    goes through — on `spawn_blocking`, which the trait states as its contract.
//!    So no runtime worker is parked on the network and landmine #1 of
//!    `docs/backend-architecture.md` does not apply. An implementation that broke
//!    that contract and blocked inline would make this the wrong shape again.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use arbor_be::BackendAppCtx;
use arbor_core::prelude::AppCtx;
use arbor_scheduler::prelude::{
    ArcAction, FnAction, ScheduleKey, ScheduleOpts, Scheduler, Trigger,
};
use garrulus_core::prelude::{load_config, GarrulusState, SyncState};
use serde_json::json;

use crate::sync;

/// Topic the frontend listens on. The payload is
/// `{ "state": <SyncState>, "toast": null | "lost" | "regained" }`.
const TOPIC: &str = "garrulus:sync-state";

/// Scheduler coordinates. One probe per process — there is one open vault.
const NAMESPACE: &str = "garrulus";
const NAME: &str = "sync-probe";

/// How long after the `Hello` handshake the first probe fires. Long enough that
/// opening the window is never competing with a network round trip, short enough
/// that the sync button is not stale by the time the user looks at it.
const INITIAL_DELAY: Duration = Duration::from_secs(3);

/// Floor on the configured cadence. A hand-edited `sync_probe_secs = 1` would
/// hammer a provider's rate limit for no benefit — nothing changes that fast.
const MIN_INTERVAL_SECS: u32 = 5;

/// The engine, kept alive for the process: `Drop for Scheduler` stops every
/// schedule it owns, so letting this go out of scope would silently end the probe.
static SCHEDULER: OnceLock<Arc<Scheduler>> = OnceLock::new();

/// Whether the Garrulus window has focus, as last reported by the frontend.
///
/// Defaults to `true` — the same posture the shell takes for its own
/// `app_focused` flag: fire normally until the frontend has actually said
/// otherwise, rather than sitting idle waiting for a message that may never come.
static FOCUSED: AtomicBool = AtomicBool::new(true);

/// The last state emitted, so an unchanged answer costs nothing.
static LAST: LazyLock<Mutex<Option<SyncState>>> = LazyLock::new(|| Mutex::new(None));

/// Consecutive unreachable ticks, and whether this episode has been announced.
/// One bool gates both halves of the toast policy — see [`toast_for`].
static OFFLINE_STREAK: AtomicU32 = AtomicU32::new(0);
static LOSS_ANNOUNCED: AtomicBool = AtomicBool::new(false);

/// Tell the backend whether the Garrulus window has focus.
///
/// Exists because a headless backend cannot see a window: without this the
/// `sync_probe_focus_only` preference would have nothing to read and would be a
/// setting that does nothing. Cheap enough to send on every focus transition.
#[arbor_rpc::handler]
fn garrulus_set_focus(_state: &GarrulusState, focused: bool) -> Result<(), String> {
    FOCUSED.store(focused, Ordering::Relaxed);
    Ok(())
}

/// Build the engine and register the probe.
///
/// Called from `main`'s post-`Hello` hook, never before it: an event emitted
/// ahead of the handshake frame makes the shell reject the connection (landmine
/// #4 of `docs/backend-architecture.md`), and this schedule's whole output is
/// events.
pub(crate) fn start(rt: tokio::runtime::Handle) {
    let state = match crate::state_arc() {
        Ok(state) => state,
        Err(e) => {
            eprintln!("garrulus-be: {e}");
            return;
        }
    };
    let scheduler = SCHEDULER.get_or_init(|| {
        let ctx: Arc<dyn AppCtx> = Arc::new(BackendAppCtx::new(state.event_sink(), rt.clone()));
        Arc::new(Scheduler::new(ctx, rt))
    });
    register(scheduler);
}

/// Re-read the cadence after the user changed it in settings.
///
/// A no-op before [`start`] has run. Re-registering the same key replaces the
/// entry, so this covers "0 → 60" (the probe was never registered) as well as
/// "60 → 300", without the caller having to know which case it is in.
pub(crate) fn reconfigure() {
    if let Some(scheduler) = SCHEDULER.get() {
        register(scheduler);
    }
}

/// Forget everything remembered about the remote's standing.
///
/// Called whenever the destination itself changes: the last state and the
/// loss-episode flag describe a remote that is no longer installed, and carrying
/// them over would suppress the first emit for the new one (or announce a
/// recovery that never happened).
pub(crate) fn forget() {
    if let Ok(mut slot) = LAST.lock() {
        *slot = None;
    }
    OFFLINE_STREAK.store(0, Ordering::Relaxed);
    LOSS_ANNOUNCED.store(false, Ordering::Relaxed);
}

// ── Registration ──────────────────────────────────────────────────────────────

/// (Re-)register the probe at the configured cadence, or cancel it when the user
/// has turned it off.
fn register(scheduler: &Arc<Scheduler>) {
    let Some(trigger) = trigger_for(load_config().sync_probe_secs) else {
        scheduler.cancel(&key());
        eprintln!("garrulus-be: the background sync probe is off (sync_probe_secs = 0)");
        return;
    };
    let Ok(state) = crate::state_arc() else { return };

    let action: ArcAction = Arc::new(FnAction(move || {
        let state = Arc::clone(&state);
        // Awaited on the scheduler's runtime, with no lock held and no thread
        // parked on the network — the remote's own `spawn_blocking` owns the
        // blocking half (see gotcha 2 in the module note).
        async move { tick(&state).await }
    }));

    if let Err(e) = scheduler.register(key(), trigger, opts(), action) {
        eprintln!("garrulus-be: could not register the sync probe: {e}");
    }
}

/// This schedule's identity. One vault per process, so one key.
fn key() -> ScheduleKey {
    ScheduleKey::new(NAMESPACE, NAME)
}

/// The cadence, or `None` when the user has disabled the probe.
///
/// `FixedDelay` rather than `FixedRate`: the gap is measured from the *end* of the
/// last probe, so a slow network throttles the loop instead of queueing ticks
/// behind it.
fn trigger_for(secs: u32) -> Option<Trigger> {
    (secs > 0).then(|| Trigger::FixedDelay {
        delay: Duration::from_secs(u64::from(secs.max(MIN_INTERVAL_SECS))),
    })
}

/// The per-tick knobs.
///
/// `only_when_focused` stays `false` on purpose — see the module note; the focus
/// preference is honoured by the gate instead, which reads a signal a headless
/// process can actually have.
fn opts() -> ScheduleOpts {
    ScheduleOpts {
        initial_delay: INITIAL_DELAY,
        only_when_focused: false,
        gate: Some(Arc::new(|| {
            !load_config().sync_probe_focus_only || FOCUSED.load(Ordering::Relaxed)
        })),
        ..ScheduleOpts::default()
    }
}

// ── The tick ──────────────────────────────────────────────────────────────────

/// One probe, and the event it may be worth.
///
/// Read-only: `probe` is the single `SyncRemote` method that cannot commit, pull
/// or push, and nothing else is called here. If this function ever grows a second
/// remote call, that is the bug.
async fn tick(state: &GarrulusState) {
    let (probed, unreachable) = match sync::probe_state(state).await {
        Ok(probed) => (probed, probed == SyncState::Offline),
        Err(e) => {
            // Silent retry: a probe that failed is a probe, not an incident. The
            // stderr line is for the log; the button just goes grey.
            eprintln!("garrulus-be: sync probe failed: {e}");
            (SyncState::Offline, true)
        }
    };
    let toast = toast_for(unreachable);
    // `changed` records as it answers, so it is evaluated unconditionally.
    if !changed(probed) && toast.is_none() {
        return;
    }
    state.emit(TOPIC, json!({ "state": probed, "toast": toast }));
}

/// Whether `next` differs from the last state emitted, remembering it either way.
///
/// A poisoned slot answers `true`: an extra event is noise, a missed one is a sync
/// button showing yesterday's answer.
fn changed(next: SyncState) -> bool {
    let Ok(mut slot) = LAST.lock() else { return true };
    if slot.as_ref() == Some(&next) {
        return false;
    }
    *slot = Some(next);
    true
}

/// The connectivity toast this tick earns, per the working agreement's
/// auto-reconnect rules.
///
/// * The first unreachable tick is **silent** — one missed probe is a dropped
///   packet, not an outage, and a toast for it would train the user to ignore
///   toasts.
/// * The loss is announced **once per episode**, on the second consecutive miss.
/// * A recovery is only news to someone who was told about the loss, so
///   `"regained"` is gated on the very same flag.
fn toast_for(unreachable: bool) -> Option<&'static str> {
    if unreachable {
        let streak = OFFLINE_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
        if streak < 2 {
            return None;
        }
        // Only marked as announced once it actually is: setting the flag on the
        // silent first miss would swallow the announcement it is meant to gate.
        return (!LOSS_ANNOUNCED.swap(true, Ordering::Relaxed)).then_some("lost");
    }
    OFFLINE_STREAK.store(0, Ordering::Relaxed);
    LOSS_ANNOUNCED.swap(false, Ordering::Relaxed).then_some("regained")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statics are process-global, so the toast tests share one lock rather
    /// than racing each other under `cargo test`'s thread pool.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        OFFLINE_STREAK.store(0, Ordering::Relaxed);
        LOSS_ANNOUNCED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_disabled_cadence_has_no_trigger_and_a_typo_is_clamped() {
        assert!(trigger_for(0).is_none());
        match trigger_for(1) {
            Some(Trigger::FixedDelay { delay }) => {
                assert_eq!(delay, Duration::from_secs(u64::from(MIN_INTERVAL_SECS)));
            }
            other => panic!("expected a clamped FixedDelay, got {other:?}"),
        }
        match trigger_for(60) {
            Some(Trigger::FixedDelay { delay }) => assert_eq!(delay, Duration::from_secs(60)),
            other => panic!("expected FixedDelay(60s), got {other:?}"),
        }
    }

    #[test]
    fn the_first_miss_is_silent_and_the_loss_is_announced_once() {
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        reset();
        assert_eq!(toast_for(true), None, "one missed probe is not an outage");
        assert_eq!(toast_for(true), Some("lost"));
        assert_eq!(toast_for(true), None, "one announcement per episode");
        assert_eq!(toast_for(true), None);
    }

    #[test]
    fn a_recovery_is_only_announced_to_someone_who_heard_the_loss() {
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        reset();
        // A blip that never earned a toast must not earn one on the way back.
        assert_eq!(toast_for(true), None);
        assert_eq!(toast_for(false), None);

        reset();
        assert_eq!(toast_for(true), None);
        assert_eq!(toast_for(true), Some("lost"));
        assert_eq!(toast_for(false), Some("regained"));
        assert_eq!(toast_for(false), None, "still fine is not news");
    }

    #[test]
    fn an_episode_can_repeat_after_a_recovery() {
        let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        reset();
        assert_eq!(toast_for(true), None);
        assert_eq!(toast_for(true), Some("lost"));
        assert_eq!(toast_for(false), Some("regained"));
        // The streak was reset, so the next outage is silent again on its first
        // tick — the policy is per episode, not per process.
        assert_eq!(toast_for(true), None);
        assert_eq!(toast_for(true), Some("lost"));
    }
}
