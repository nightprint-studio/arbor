//! Changes made to `.java` files behind the editor's back, fed into the index.
//!
//! The Project tree's watcher ([`crate::tree_watch`]) already hears every `git checkout`, branch
//! switch and generator run, but only the tree and the completion statistics listened. The symbol
//! index and the reference index learned about such a change at the next validation, so in between,
//! find-usages and completion described the files as they were before the checkout.
//!
//! ## Shape
//!
//! - The watcher thread does nothing but [`java_changed`] / [`dir_changed`]: a lock, a set insert, a
//!   notify. All reading, hashing and parsing happens on this module's own worker thread, so a burst
//!   of events can never slow the watcher down or make it drop one.
//! - **Debounced with a ceiling**, like the tree: changes accumulate until the burst has been quiet for
//!   [`QUIET`], and a burst that never goes quiet is still flushed after [`MAX_WAIT`]. A checkout that
//!   touches a thousand files is one batch, handed to [`IndexService::sync_disk_changes`] in one call.
//! - **The editor's own saves do not count.** A save is a disk event for a file the index already
//!   holds, byte for byte; `sync_disk_changes` compares content hashes, so that echo is a no-op, and it
//!   leaves alone any file the editor sent an edit for within [`EDITOR_ECHO_WINDOW`] (see
//!   [`crate::edit_order`]) — there the buffer is newer than the disk, and indexing the disk would roll
//!   the index back.
//! - A project that is still building hands its paths back; they are offered again, a bounded number of
//!   times, so a change that lands during the open is not lost.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

use crate::index_service::{DiskSync, IndexService};

/// How long the disk must be quiet before a burst is applied.
const QUIET: Duration = Duration::from_millis(700);

/// The longest a burst is held, quiet or not.
const MAX_WAIT: Duration = Duration::from_secs(4);

/// A file the editor sent an edit for this recently is not indexed from disk: its buffer is newer.
/// Wider than [`QUIET`] plus the editor's own debounce, so the echo of a save cannot outrun it.
pub(crate) const EDITOR_ECHO_WINDOW: Duration = Duration::from_secs(3);

/// How many times a project that was still building has its paths offered again (about a second each).
const MAX_DEFERRALS: u32 = 120;

/// The paths waiting to be applied, with the two clocks that decide when.
#[derive(Debug, Default)]
pub(crate) struct ChangeBatch {
    pending: BTreeSet<PathBuf>,
    /// When the oldest pending change arrived.
    first: Option<Instant>,
    /// When the newest one did.
    last: Option<Instant>,
}

impl ChangeBatch {
    /// Record a changed path. Repeats collapse into one entry.
    pub(crate) fn note(&mut self, path: PathBuf, now: Instant) {
        self.pending.insert(path);
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// How long until the batch is due: `None` when nothing is pending, zero when it is due now.
    ///
    /// Due when the changes have stopped for `quiet`, or when the oldest has waited `max_wait` — the
    /// ceiling that keeps a never-quiet tree (a build writing into it) from holding the batch forever.
    pub(crate) fn due_in(&self, now: Instant, quiet: Duration, max_wait: Duration) -> Option<Duration> {
        let (first, last) = (self.first?, self.last?);
        let quiet_left = quiet.saturating_sub(now.saturating_duration_since(last));
        let ceiling_left = max_wait.saturating_sub(now.saturating_duration_since(first));
        Some(quiet_left.min(ceiling_left))
    }

    /// Hand over everything pending and start the next burst clean.
    pub(crate) fn take(&mut self) -> Vec<PathBuf> {
        self.first = None;
        self.last = None;
        std::mem::take(&mut self.pending).into_iter().collect()
    }
}

/// The batch and the signal that wakes the worker when it grows.
static QUEUE: LazyLock<(Mutex<ChangeBatch>, Condvar)> =
    LazyLock::new(|| (Mutex::new(ChangeBatch::default()), Condvar::new()));

/// Starts the worker on the first change, so a window that never sees one runs no thread.
static WORKER: Once = Once::new();

/// A `.java` file under `root` was created, changed, removed or moved (`rel` is forward-slashed).
pub(crate) fn java_changed(root: &Path, rel: &str) {
    enqueue(root.join(rel));
}

/// A directory under `root` appeared, vanished or moved; the files in it were not listed one by one.
pub(crate) fn dir_changed(root: &Path, rel: &str) {
    enqueue(root.join(rel));
}

fn enqueue(path: PathBuf) {
    WORKER.call_once(spawn_worker);
    let (lock, signal) = &*QUEUE;
    lock.lock().unwrap_or_else(|p| p.into_inner()).note(path, Instant::now());
    signal.notify_one();
}

fn spawn_worker() {
    let started = std::thread::Builder::new().name("bennu-external-sync".to_string()).spawn(run_worker);
    if let Err(e) = started {
        // Without the worker the index only learns of such changes at the next validation, as before.
        eprintln!("bennu-be: external change sync did not start ({e})");
    }
}

/// Wait for a due batch, apply it, repeat. Never returns.
fn run_worker() {
    let mut deferrals = 0u32;
    loop {
        let paths = next_due_batch();
        // A panic in one batch (an odd source, a damaged cache) must not end the worker for good.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| IndexService::global().sync_disk_changes(paths)))
            .unwrap_or_else(|_| {
                eprintln!("bennu-be: external change sync crashed — see the backend log");
                DiskSync::default()
            });
        if outcome.deferred.is_empty() {
            deferrals = 0;
            continue;
        }
        deferrals += 1;
        if deferrals >= MAX_DEFERRALS {
            eprintln!("bennu-be: project never became ready — {} changed path(s) left to validation", outcome.deferred.len());
            deferrals = 0;
            continue;
        }
        let (lock, _) = &*QUEUE;
        let mut batch = lock.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        for path in outcome.deferred {
            batch.note(path, now);
        }
    }
}

/// Block until a batch is due and take it.
fn next_due_batch() -> Vec<PathBuf> {
    let (lock, signal) = &*QUEUE;
    let mut batch = lock.lock().unwrap_or_else(|p| p.into_inner());
    loop {
        match batch.due_in(Instant::now(), QUIET, MAX_WAIT) {
            None => batch = signal.wait(batch).unwrap_or_else(|p| p.into_inner()),
            Some(left) if left.is_zero() => return batch.take(),
            Some(left) => batch = signal.wait_timeout(batch, left).unwrap_or_else(|p| p.into_inner()).0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Duration = Duration::from_millis(700);
    const M: Duration = Duration::from_secs(4);

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn an_empty_batch_is_never_due() {
        assert_eq!(ChangeBatch::default().due_in(Instant::now(), Q, M), None);
    }

    #[test]
    fn a_burst_becomes_one_batch_once_it_goes_quiet() {
        let base = Instant::now();
        let mut batch = ChangeBatch::default();
        for i in 0..1000u64 {
            batch.note(PathBuf::from(format!("/p/F{i}.java")), at(base, i / 10));
        }
        // 100 ms in, the newest change is 1 ms old: far from quiet.
        assert!(batch.due_in(at(base, 100), Q, M).unwrap() > Duration::ZERO);
        // 700 ms after the last change it is due.
        assert_eq!(batch.due_in(at(base, 99 + 700), Q, M), Some(Duration::ZERO));
        let taken = batch.take();
        assert_eq!(taken.len(), 1000, "one batch for the whole burst");
        assert_eq!(batch.due_in(at(base, 5000), Q, M), None, "taking starts the next burst clean");
    }

    #[test]
    fn repeated_events_for_one_file_collapse() {
        let base = Instant::now();
        let mut batch = ChangeBatch::default();
        for ms in 0..5 {
            batch.note(PathBuf::from("/p/A.java"), at(base, ms));
        }
        assert_eq!(batch.take(), vec![PathBuf::from("/p/A.java")]);
    }

    #[test]
    fn each_new_change_pushes_the_quiet_deadline_out() {
        let base = Instant::now();
        let mut batch = ChangeBatch::default();
        batch.note(PathBuf::from("/p/A.java"), at(base, 0));
        let before = batch.due_in(at(base, 600), Q, M).unwrap();
        batch.note(PathBuf::from("/p/B.java"), at(base, 600));
        let after = batch.due_in(at(base, 600), Q, M).unwrap();
        assert!(after > before, "{before:?} then {after:?}");
    }

    #[test]
    fn a_tree_that_never_goes_quiet_is_still_flushed_at_the_ceiling() {
        let base = Instant::now();
        let mut batch = ChangeBatch::default();
        // A change every 100 ms for five seconds.
        for ms in (0..5000).step_by(100) {
            batch.note(PathBuf::from(format!("/p/F{ms}.java")), at(base, ms));
        }
        // 4 s after the first change it is due, though the last one is only 100 ms old.
        assert_eq!(batch.due_in(at(base, 4000), Q, M), Some(Duration::ZERO));
        // …and not before.
        assert!(batch.due_in(at(base, 3000), Q, M).unwrap() > Duration::ZERO);
    }
}
