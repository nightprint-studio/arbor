//! Per-file ordering of live-edit patches.
//!
//! `bennu_did_change` is dispatched on a pool thread per request, with nothing that orders two
//! requests for the same file. The editor debounces at 400 ms, but a patch is not bounded by that: a
//! slow one (a big file, a busy machine) can still be running when the next edit's request arrives, and
//! if the newer patch finishes first the older one then overwrites it — the index ends up holding text
//! the user has already replaced, and stays that way until the next keystroke.
//!
//! Each request therefore takes a **ticket** when it arrives, and the patch itself runs under a
//! per-file lock that first checks the ticket is still the newest. A request that finds a newer one
//! waiting does nothing at all: the newer patch carries the newer full text, so skipping the older
//! one loses nothing (every request carries the whole buffer, not a delta).
//!
//! The same record tells the external-change watcher when the editor last touched a file, so a disk
//! event that is merely the echo of the editor's own save cannot roll the index back to what is on
//! disk while the buffer has moved on — see [`crate::external_sync`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// One file's ordering state.
#[derive(Default)]
struct FileEdits {
    /// The newest ticket handed out for this file.
    latest: AtomicU64,
    /// Held while a patch for this file runs, so two patches never interleave.
    apply: Mutex<()>,
    /// When a request for this file last arrived.
    last_edit: Mutex<Option<Instant>>,
}

/// The ordering state of every file the editor has sent an edit for.
#[derive(Default)]
pub(crate) struct EditOrder {
    files: Mutex<HashMap<String, Arc<FileEdits>>>,
}

static GLOBAL: LazyLock<EditOrder> = LazyLock::new(EditOrder::default);

/// A place in one file's queue of edits. Taken when the request arrives, redeemed by
/// [`run_if_current`](Self::run_if_current).
pub(crate) struct Ticket {
    file: Arc<FileEdits>,
    number: u64,
}

impl EditOrder {
    /// The process-wide ordering.
    pub(crate) fn global() -> &'static EditOrder {
        &GLOBAL
    }

    /// Take the next ticket for `file` and note that the editor just touched it.
    pub(crate) fn begin(&self, file: &str) -> Ticket {
        let entry = {
            let mut files = self.files.lock().unwrap_or_else(|p| p.into_inner());
            Arc::clone(files.entry(key_of(file)).or_default())
        };
        // Numbered before anything else can run for this file, so ticket order is arrival order.
        let number = entry.latest.fetch_add(1, Ordering::SeqCst) + 1;
        *entry.last_edit.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
        Ticket { file: entry, number }
    }

    /// Whether the editor sent an edit for `file` within `window` before `now`.
    pub(crate) fn edited_within(&self, file: &str, now: Instant, window: Duration) -> bool {
        let entry = {
            let files = self.files.lock().unwrap_or_else(|p| p.into_inner());
            files.get(&key_of(file)).cloned()
        };
        let Some(entry) = entry else { return false };
        let last = *entry.last_edit.lock().unwrap_or_else(|p| p.into_inner());
        last.is_some_and(|t| now.saturating_duration_since(t) < window)
    }
}

impl Ticket {
    /// Run `patch` unless a newer request for the same file has arrived by now. Returns whether it ran.
    ///
    /// Waits for any patch already running for the file first, then re-checks: a request that was the
    /// newest when it arrived but was overtaken while it waited is dropped here, not applied late.
    pub(crate) fn run_if_current(self, patch: impl FnOnce()) -> bool {
        let _turn = self.file.apply.lock().unwrap_or_else(|p| p.into_inner());
        if self.file.latest.load(Ordering::SeqCst) != self.number {
            return false;
        }
        patch();
        true
    }
}

/// The key a file is tracked under: forward slashes, as the frontend sends them and the index keys.
fn key_of(file: &str) -> String {
    file.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_ticket_runs_and_an_overtaken_one_does_not() {
        let order = EditOrder::default();
        let old = order.begin("/p/A.java");
        let new = order.begin("/p/A.java");
        let mut ran = Vec::new();
        assert!(!old.run_if_current(|| ran.push("old")), "an older edit must not overwrite a newer one");
        assert!(new.run_if_current(|| ran.push("new")));
        assert_eq!(ran, vec!["new"]);
    }

    #[test]
    fn files_are_ordered_independently() {
        let order = EditOrder::default();
        let a = order.begin("/p/A.java");
        let b = order.begin("/p/B.java");
        assert!(a.run_if_current(|| {}));
        assert!(b.run_if_current(|| {}));
    }

    #[test]
    fn a_ticket_with_nothing_newer_always_runs() {
        let order = EditOrder::default();
        for _ in 0..3 {
            assert!(order.begin("/p/A.java").run_if_current(|| {}));
        }
    }

    #[test]
    fn separators_do_not_split_one_file_into_two() {
        let order = EditOrder::default();
        let first = order.begin("C:\\p\\A.java");
        let second = order.begin("C:/p/A.java");
        assert!(!first.run_if_current(|| {}));
        assert!(second.run_if_current(|| {}));
    }

    #[test]
    fn a_slow_older_patch_cannot_land_after_a_newer_one() {
        // The race this exists for: the older request starts first and is slow; the newer one arrives
        // meanwhile. Whatever the interleaving, the last write must be the newer text.
        let order = Arc::new(EditOrder::default());
        let applied = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let old = order.begin("/p/A.java");
        let new = order.begin("/p/A.java");

        let slow = {
            let applied = Arc::clone(&applied);
            std::thread::spawn(move || {
                old.run_if_current(|| {
                    std::thread::sleep(Duration::from_millis(50));
                    applied.lock().unwrap().push("old");
                });
            })
        };
        new.run_if_current(|| applied.lock().unwrap().push("new"));
        slow.join().unwrap();

        let applied = applied.lock().unwrap();
        assert_eq!(applied.last(), Some(&"new"), "applied: {applied:?}");
    }

    #[test]
    fn edited_within_reports_only_recent_edits() {
        let order = EditOrder::default();
        assert!(!order.edited_within("/p/A.java", Instant::now(), Duration::from_secs(5)));
        let _ticket = order.begin("/p/A.java");
        let now = Instant::now();
        assert!(order.edited_within("/p/A.java", now, Duration::from_secs(5)));
        assert!(!order.edited_within("/p/A.java", now + Duration::from_secs(10), Duration::from_secs(5)));
        assert!(!order.edited_within("/p/B.java", now, Duration::from_secs(5)));
    }
}
