//! `watch` — the filesystem watcher over the open vault.
//!
//! Notes change under the app all the time and it is normal, not exceptional: a
//! sync pull rewrites files, Obsidian may be open on the same folder, and the
//! other PC's changes arrive as a burst. The frontend needs to know, and it needs
//! to know *once* per burst.
//!
//! ## Shape, and why it is this shape
//!
//! - **Its own OS thread**, not a tokio task. `notify`'s callback is synchronous
//!   and the debounce loop parks on a timeout; doing that on a runtime worker is
//!   landmine #1 in `docs/backend-architecture.md`. The thread owns the watcher,
//!   so dropping the thread drops the watcher.
//! - **Debounced**: paths accumulate until the vault has been quiet for the
//!   configured window, then go out as one `garrulus:vault-changed` event. A pull
//!   touching forty notes is one event, not forty.
//! - **Filtered**: `.arbor/` and `.git/` changes are Garrulus's or git's own
//!   bookkeeping and are not "a note changed".
//! - **Backend-local state**, not a field on `GarrulusState`: exactly one vault is
//!   open per process, the handle is an OS resource this binary owns, and the
//!   Tauri-free core has no business holding a `notify` type.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use notify::{RecursiveMode, Watcher};
use serde_json::json;

use crate::vault_io;

/// Topic the frontend listens on. The payload is
/// `{ "root": "<vault>", "paths": ["notes/a.md", …] }`.
const TOPIC: &str = "garrulus:vault-changed";

/// How often the debounce loop wakes to check whether the burst has ended. Small
/// enough to be invisible, large enough not to spin.
const TICK: Duration = Duration::from_millis(50);

/// Upper bound on paths carried in one event. Past this the frontend is going to
/// reload the tree wholesale anyway, so the list stops being worth sending.
const MAX_PATHS: usize = 200;

/// The running watcher, if any. One vault per process → one slot.
static CURRENT: LazyLock<Mutex<Option<Running>>> = LazyLock::new(|| Mutex::new(None));

/// A live watcher thread and the flag that asks it to stop.
struct Running {
    root: PathBuf,
    stop: Arc<AtomicBool>,
}

/// Start watching `root`, replacing any watcher already running.
///
/// A no-op when the same root is already being watched, so re-opening the same
/// vault does not stack threads. Errors only when `notify` cannot watch the path
/// at all — the caller treats that as "no live updates", not as a failed open.
pub fn start(sink: Arc<dyn EventSink>, root: PathBuf, debounce_ms: u64) -> Result<(), String> {
    let mut slot = CURRENT.lock().map_err(|_| "the watcher slot is poisoned".to_string())?;
    if slot.as_ref().is_some_and(|r| r.root == root) {
        return Ok(());
    }
    if let Some(previous) = slot.take() {
        previous.stop.store(true, Ordering::Relaxed);
    }

    let stop = Arc::new(AtomicBool::new(false));
    spawn_thread(sink, root.clone(), Duration::from_millis(debounce_ms), Arc::clone(&stop))?;
    *slot = Some(Running { root, stop });
    Ok(())
}

/// Ask the running watcher to stop. Safe to call when none is running (vault
/// close, process shutdown).
pub fn stop() {
    if let Ok(mut slot) = CURRENT.lock() {
        if let Some(running) = slot.take() {
            running.stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Build the watcher and run the debounce loop on a dedicated OS thread.
///
/// The watcher is constructed **on** that thread and never leaves it, so its
/// lifetime is the thread's: when the stop flag ends the loop, the watcher drops
/// and the OS handle goes with it.
fn spawn_thread(
    sink: Arc<dyn EventSink>,
    root: PathBuf,
    debounce: Duration,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    // Bounded by nothing on purpose: the channel is drained every TICK, and a
    // dropped event would mean a change the frontend never hears about.
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        // The receiver is gone once the loop ends; a send failure then is the
        // normal way this callback learns it is over.
        let _ = tx.send(res);
    })
    .map_err(|e| format!("watcher: {e}"))?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| format!("watcher on {}: {e}", root.display()))?;

    std::thread::Builder::new()
        .name("garrulus-watch".to_string())
        .spawn(move || {
            // Moved in so it lives exactly as long as the loop.
            let _watcher = watcher;
            let mut pending: BTreeSet<String> = BTreeSet::new();
            let mut last_change: Option<Instant> = None;

            while !stop.load(Ordering::Relaxed) {
                if let Ok(Ok(event)) = rx.recv_timeout(TICK) {
                    for path in event.paths {
                        if let Some(rel) = interesting(&root, &path) {
                            pending.insert(rel);
                        }
                    }
                    if !pending.is_empty() {
                        last_change = Some(Instant::now());
                    }
                }
                // Flush once the burst has been quiet for the debounce window.
                if last_change.is_some_and(|t| t.elapsed() >= debounce) {
                    flush(&sink, &root, &mut pending);
                    last_change = None;
                }
            }
            // Whatever was still pending at close is worth one last event: the
            // frontend would otherwise show a tree that is one burst stale.
            flush(&sink, &root, &mut pending);
        })
        .map_err(|e| format!("watcher thread: {e}"))?;
    Ok(())
}

/// Emit the accumulated paths and clear them.
fn flush(sink: &Arc<dyn EventSink>, root: &Path, pending: &mut BTreeSet<String>) {
    if pending.is_empty() {
        return;
    }
    let truncated = pending.len() > MAX_PATHS;
    let paths: Vec<String> = pending.iter().take(MAX_PATHS).cloned().collect();
    pending.clear();
    sink.emit(
        TOPIC,
        json!({
            "root":      root.to_string_lossy(),
            "paths":     paths,
            "truncated": truncated,
        }),
    );
}

/// The vault-relative path of a change worth reporting, or `None` for one that is
/// not (outside the vault, or Garrulus's / git's own bookkeeping).
///
/// Attachments are deliberately included: a pasted image landing in the
/// attachments folder is a change the note view has to notice.
fn interesting(root: &Path, path: &Path) -> Option<String> {
    let rel = vault_io::to_rel(root, path)?;
    if vault_io::is_internal(&rel) {
        return None;
    }
    Some(rel)
}
