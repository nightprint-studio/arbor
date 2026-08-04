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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;

use crate::classpath_stamp::classpath_epoch;
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

/// One pass: re-stamp every built project and reload the ones whose jars moved.
fn tick() {
    let service = IndexService::global();
    for (root, jdk, jars) in service.classpath_snapshot() {
        if jars.is_empty() {
            continue;
        }
        let epoch = classpath_epoch(&jdk, &jars);

        let previous = {
            let mut guard = match SEEN.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.get_or_insert_with(HashMap::new).insert(root.clone(), epoch)
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Two projects opening at the same moment must not each start a watcher.
    #[test]
    fn the_watcher_starts_at_most_once() {
        RUNNING.store(false, Ordering::SeqCst);
        assert!(!RUNNING.swap(true, Ordering::SeqCst), "first caller wins");
        assert!(RUNNING.swap(true, Ordering::SeqCst), "second caller sees it running");
        RUNNING.store(false, Ordering::SeqCst);
    }
}
