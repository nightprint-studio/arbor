//! Noticing that a dependency was rebuilt **while Bennu is open**.
//!
//! Everything cached against the classpath is now keyed by [`classpath_epoch`], so nothing
//! survives a restart wrongly. What that does not cover is the session you are in: the
//! dependency member tier is in memory by design (a persistent one re-serialized the whole
//! map every 128 decoded classes — the CPU-pegging regression `ClasspathIndex` documents),
//! so after `mvn install` on a module you are working on, Bennu keeps resolving against the
//! classes it decoded before you rebuilt. Which is precisely the workflow this matters in:
//! nobody reinstalls a dependency they are not editing.
//!
//! So the epoch is recomputed on a timer, and a change rebuilds the project's index and
//! tells the frontend. **Polling and not a filesystem watcher**: it is one `stat` per jar,
//! it needs no new dependency, and it cannot miss a change the way a watcher misses events
//! delivered while nothing was listening. The interval is long enough to be invisible and
//! short enough that a rebuild is picked up before you have finished switching windows.
//!
//! ## The poms are watched too, and for the opposite reason
//!
//! The jar epoch answers "did a dependency's **contents** change". It is structurally blind to
//! the other half — "did the project change its **mind** about which dependencies it has" —
//! because adding a `<dependency>` to a pom does not touch a single jar Bennu already resolved.
//! The list is cached against the poms' newest mtime, but nothing was ever *asking* it again
//! inside a live session, so the answer was: edit a pom, and Bennu carries on with the classpath
//! it opened with until the project is reopened or the Maven ▸ Reload button is pressed.
//!
//! So each tick stamps the poms as well, and a change there triggers a **reindex** rather than a
//! classpath reload — the jar list itself has to be resolved again, which is exactly what
//! `IndexService::reindex` drops the cache to force.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;

use crate::classpath_stamp::classpath_epoch;
use crate::dep_classpath::poms_mtime;
use crate::index_service::IndexService;

/// Emitted when a project's dependency jars changed on disk and its index has been
/// rebuilt. The frontend reloads what it read from the old classpath.
pub const EVT_CLASSPATH_CHANGED: &str = "arbor://bennu/classpath-changed";

/// How often the resolved jars are re-stamped.
///
/// A Maven install takes tens of seconds, so this does not need to be quick to feel
/// immediate — it needs to have happened by the time you switch back to the editor. Ten
/// seconds against a few hundred jars is a few hundred `stat`s a minute, which is nothing,
/// and long enough that a build writing its jars finds them settled rather than half-copied.
const INTERVAL: Duration = Duration::from_secs(10);

static RUNNING: AtomicBool = AtomicBool::new(false);

/// The epoch each root was last seen at. Not a field on the project slot: this is the
/// watcher's own memory of what it has already reacted to, and a slot rebuilt for an
/// unrelated reason should not silently reset it.
static SEEN: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

/// The newest pom mtime each root was last seen at — the same memory, for the other question.
static SEEN_POMS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

/// Record `value` for `root` in one of the watcher's memories, returning what was there before.
/// `None` means this is the first sight of the project, which is a baseline and not a change.
fn remember(memory: &Mutex<Option<HashMap<String, u64>>>, root: &str, value: u64) -> Option<u64> {
    let mut guard = match memory.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.get_or_insert_with(HashMap::new).insert(root.to_string(), value)
}

/// What a memory holds for `root`, without writing to it.
///
/// The poms need this and the jars do not, and the difference is the settle rule: a stamp that
/// changed but is too fresh to act on must stay **unrecorded**, or the next tick would compare it
/// against itself, find nothing changed, and drop the edit on the floor. Recording is therefore
/// something the pom path does when it acts, not when it looks.
fn peek(memory: &Mutex<Option<HashMap<String, u64>>>, root: &str) -> Option<u64> {
    let guard = match memory.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.as_ref()?.get(root).copied()
}

/// Start the watcher, once. Called from every project open, so it begins with the first
/// project and is a no-op for the rest.
pub fn ensure_running() {
    // `swap` and not `load`+`store`: two projects opening at once would otherwise both see
    // "not running" and start a thread each.
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("bennu-classpath-watch".to_string())
        .spawn(|| loop {
            std::thread::sleep(INTERVAL);
            tick();
        })
        .map(|_| ())
        // A thread that cannot be spawned costs the in-session reload, not the session.
        // Reset the flag so a later open tries again.
        .unwrap_or_else(|_| RUNNING.store(false, Ordering::SeqCst));
}

/// One pass: re-stamp every built project, and react to whichever of the two things moved.
fn tick() {
    let service = IndexService::global();
    for (root, jdk, jars) in service.classpath_snapshot() {
        // The poms first, and it is the more drastic of the two: a pom edit can change **which**
        // jars the project wants, so re-resolving is the answer and reloading the ones already in
        // hand is not. Checked even for a project with no resolved jars — that is precisely the
        // state a first `<dependency>` is added from.
        if let Some(stamp) = poms_mtime(std::path::Path::new(&root)) {
            match peek(&SEEN_POMS, &root) {
                // First sight: the baseline. It was just indexed against exactly these poms.
                None => {
                    remember(&SEEN_POMS, &root, stamp);
                }
                // Settled, not merely changed. A pom is edited in the editor next door, and an
                // edit that is still being typed gets saved several times in a row — reacting to
                // the first save would start a whole-project reindex against a half-written
                // `<dependency>`, and then another against the next one. Waiting one interval
                // costs ten seconds on a job that takes longer than that anyway, and the stamp
                // stays UNRECORDED until we act, so a change seen too early is not forgotten.
                Some(previous) if previous != stamp && settled(stamp) => {
                    let Some(sink) = service.sink() else { continue };
                    remember(&SEEN_POMS, &root, stamp);
                    eprintln!("bennu: a pom changed under {root} — re-resolving the classpath");
                    crate::library_beans::forget(&root);
                    // Reindex, not `reload_changed_classpath`: the jar LIST is what has to be
                    // computed again, and `reindex` is the path that drops its cache to force it.
                    service.reindex(&root, std::sync::Arc::clone(&sink));
                    // The jar epoch it is about to be indexed against is not the one this loop
                    // recorded a moment ago, so forget it and let the next tick take the baseline.
                    remove_seen(&root);
                    sink.emit(EVT_CLASSPATH_CHANGED, json!({ "root": root }));
                    continue;
                }
                Some(_) => {}
            }
        }

        if jars.is_empty() {
            continue;
        }
        let epoch = classpath_epoch(&jdk, &jars);
        let previous = remember(&SEEN, &root, epoch);

        // First sight of a project is the baseline, not a change: it was just indexed
        // against exactly these jars, and reloading here would rebuild every project once
        // for nothing on the first tick after opening.
        let Some(previous) = previous else { continue };
        if previous == epoch {
            continue;
        }

        let Some(sink) = service.sink() else { continue };
        eprintln!("bennu: dependency jars changed under {root} — rebuilding the index");
        // The library-bean scan is stamped per artifact and re-reads what moved, but its
        // session cache is keyed by the allowlist alone and would keep the old answer.
        crate::library_beans::forget(&root);
        service.reload_changed_classpath(&root, std::sync::Arc::clone(&sink));
        sink.emit(EVT_CLASSPATH_CHANGED, json!({ "root": root }));
    }
}

/// Whether a modification `stamp` (seconds since the epoch) is old enough that nothing is likely
/// still writing it. A clock that disagrees with the filesystem reads as "settled", which is the
/// direction that reacts rather than the one that goes quiet.
fn settled(stamp: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(stamp);
    now.saturating_sub(stamp) >= INTERVAL.as_secs()
}

/// Drop the jar baseline for `root`, so the next tick takes a fresh one instead of comparing
/// against the epoch of a classpath that is being replaced.
fn remove_seen(root: &str) {
    let mut guard = match SEEN.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(map) = guard.as_mut() {
        map.remove(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first sight of a value is a baseline, not a change — otherwise every project would
    /// reload once on the first tick after opening, for nothing.
    #[test]
    fn the_first_stamp_is_a_baseline_and_the_second_is_a_comparison() {
        static M: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);
        assert_eq!(remember(&M, "/p", 1), None);
        assert_eq!(remember(&M, "/p", 1), Some(1));
        assert_eq!(remember(&M, "/p", 2), Some(1));
    }

    /// Looking must not be recording — that is the whole reason the pom path peeks. If a look
    /// wrote, a change seen before it settled would be compared against itself on the next tick
    /// and silently dropped, which is the bug this watcher exists to not have.
    #[test]
    fn peeking_does_not_record() {
        static M: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);
        assert_eq!(peek(&M, "/q"), None);
        assert_eq!(peek(&M, "/q"), None);
        remember(&M, "/q", 7);
        assert_eq!(peek(&M, "/q"), Some(7));
    }

    /// A file written a moment ago may still be being written; one written long ago is not.
    #[test]
    fn a_fresh_stamp_is_not_settled_and_an_old_one_is() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(!settled(now));
        assert!(settled(now - INTERVAL.as_secs() - 1));
    }

    /// Two projects opening at the same moment must not each start a watcher.
    #[test]
    fn the_watcher_starts_at_most_once() {
        RUNNING.store(false, Ordering::SeqCst);
        assert!(!RUNNING.swap(true, Ordering::SeqCst), "first caller wins");
        assert!(RUNNING.swap(true, Ordering::SeqCst), "second caller sees it running");
        RUNNING.store(false, Ordering::SeqCst);
    }
}
