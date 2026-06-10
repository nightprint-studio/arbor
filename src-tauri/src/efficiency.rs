//! Coalescing, off-UI-thread driver for OS power-throttling (EcoQoS).
//!
//! Applying efficiency mode means a full Toolhelp snapshot of every process on
//! the machine to find the WebView2 renderer descendants (see
//! [`crate::platform::set_efficiency_mode`]). That scan is expensive, and it
//! used to run **synchronously inside the Tauri window-event callback** — the
//! UI/event thread. On resume from standby Windows delivers a burst of
//! focus / blur / resize messages, each of which re-ran the whole-system scan
//! on the (still throttled) UI thread: the exact "Arbor freezes after the PC
//! wakes" symptom, which grew worse the longer the app sat in the background.
//!
//! This controller decouples the request from the work:
//!   * window events only set the *desired* state (a cheap atomic) and nudge a
//!     dedicated worker thread — they never run the scan;
//!   * the worker **coalesces** the resume burst (short debounce + swallow),
//!     **skips redundant scans** when the state hasn't changed, and **re-scans
//!     periodically while throttled** so renderers spawned in the background
//!     still receive EcoQoS;
//!   * the OS power-resume hook calls [`force_reapply`] so a state the power
//!     transition reset is reconciled even when no fresh focus event fires.

use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// A resume delivers many focus/resize events in quick succession; collapse
/// them into a single apply by letting the burst settle before scanning.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// While throttled, re-scan this often to catch WebView2 renderers spawned
/// after we went to the background (they would otherwise never get EcoQoS).
const RESCAN_WHILE_THROTTLED: Duration = Duration::from_secs(30);

/// Effectively "until the next event" — the worker only wakes early via the
/// condvar while focused, so the exact value just needs to be large.
const IDLE_WAIT: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct Pending {
    /// A re-apply was requested.
    wake: bool,
    /// Re-apply even if `desired == applied` (renderers may have changed, or
    /// the OS reset process priorities across a power transition).
    force: bool,
}

pub struct EfficiencyController {
    /// Desired throttle state: `true` = app unfocused, EcoQoS wanted.
    desired: AtomicBool,
    /// Last applied state, to skip redundant whole-system scans:
    /// `-1` unknown, `0` applied-off, `1` applied-on.
    applied: AtomicI8,
    pending: Mutex<Pending>,
    cv: Condvar,
}

impl EfficiencyController {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            desired: AtomicBool::new(false),
            applied: AtomicI8::new(-1),
            pending: Mutex::new(Pending::default()),
            cv: Condvar::new(),
        })
    }

    fn nudge(&self, force: bool) {
        let mut p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        p.wake = true;
        p.force |= force;
        self.cv.notify_one();
    }

    fn spawn(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("arbor-efficiency".to_string())
            .spawn(move || me.run())
            .expect("failed to spawn efficiency worker");
    }

    fn run(self: Arc<Self>) {
        loop {
            // 1. Block until work arrives (or, while throttled, until it's time
            //    to re-scan for renderers spawned in the background).
            let mut force = self.wait_for_work();

            // 2. Debounce the resume burst, then take + clear the pending
            //    request, merging anything that queued during the wait.
            std::thread::sleep(DEBOUNCE);
            {
                let mut p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
                force |= p.force;
                p.wake = false;
                p.force = false;
            }

            // 3. Apply, skipping the expensive scan when nothing changed.
            let want = self.desired.load(Ordering::Relaxed);
            let want_i8 = if want { 1 } else { 0 };
            if !force && self.applied.load(Ordering::Relaxed) == want_i8 {
                continue;
            }
            crate::platform::set_efficiency_mode(want);
            self.applied.store(want_i8, Ordering::Relaxed);
        }
    }

    /// Block until a request arrives, or — while throttled — until the
    /// periodic re-scan is due. Returns whether the wake-up implies a forced
    /// re-apply (the periodic timeout always does; the renderer set may grow).
    fn wait_for_work(&self) -> bool {
        let throttled = self.desired.load(Ordering::Relaxed);
        let timeout = if throttled { RESCAN_WHILE_THROTTLED } else { IDLE_WAIT };

        let p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if p.wake {
            // Already pending — `run()` reads `force` after the debounce.
            return false;
        }
        let (_p, res) = self
            .cv
            .wait_timeout_while(p, timeout, |p| !p.wake)
            .unwrap_or_else(|e| e.into_inner());
        // A timeout while throttled is the periodic re-scan: force a re-apply.
        res.timed_out() && throttled
    }
}

// ---------------------------------------------------------------------------
// Process-global handle
// ---------------------------------------------------------------------------

static CONTROLLER: OnceLock<Arc<EfficiencyController>> = OnceLock::new();

/// Create the controller and spawn its worker thread. Idempotent — call once
/// from Tauri's `setup`.
pub fn init() {
    let controller = CONTROLLER.get_or_init(EfficiencyController::new);
    controller.spawn();
}

/// Request a throttle state. Cheap and non-blocking — safe to call from the
/// window-event thread. `throttle == true` when the app is unfocused.
pub fn request(throttle: bool) {
    if let Some(c) = CONTROLLER.get() {
        c.desired.store(throttle, Ordering::Relaxed);
        c.nudge(false);
    }
}

/// Force a re-apply of the current desired state even if it matches what was
/// last applied. Used by the OS power-resume hook so throttling state the
/// power transition reset gets reconciled.
pub fn force_reapply() {
    if let Some(c) = CONTROLLER.get() {
        c.nudge(true);
    }
}
