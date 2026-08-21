//! The filesystem watcher behind the Project tree.
//!
//! A project changes under the editor constantly and it is normal, not exceptional: `git checkout`
//! rewrites half the tree, `cargo new` adds a crate, `npm install` creates a hundred thousand
//! files, another editor saves. Until this existed the tree reloaded only when **Bennu itself**
//! changed something — a New file, a Delete, a Rename — so everything else stayed invisible until
//! the project was reopened.
//!
//! ## Shape, and why it is this shape
//!
//! - **Its own OS thread**, not a tokio task. `notify`'s callback is synchronous and the debounce
//!   loop parks on a timeout; doing that on a runtime worker is landmine #1 in
//!   `docs/backend-architecture.md`. The thread owns the watcher, so ending the thread drops it.
//! - **Debounced.** Paths accumulate until the tree has been quiet, then leave as one event. A
//!   `git checkout` touching four hundred files is one reload, not four hundred.
//! - **`target/` and `node_modules/` are never watched at all** — not filtered afterwards,
//!   *unwatched*. This is the difference between working and not: in this repository those two
//!   hold 17 000 of the 17 500 directories, and on Linux a recursive watch is one inotify handle
//!   per directory against a limit that is regularly 8 192. Watching the root shallowly and each
//!   interesting top-level child recursively costs a few dozen handles instead.
//! - **A directory created later gets watched.** The root's own shallow watch sees it appear, and
//!   the loop adds a recursive watch for it — otherwise `mkdir src/` on a fresh project would be
//!   the last thing the tree ever heard about.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use notify::{RecursiveMode, Watcher};
use serde_json::json;

/// Topic the frontend listens on. Payload: `{ "root": …, "paths": [...], "truncated": bool }`.
pub const TOPIC: &str = "arbor://bennu/tree-changed";

/// How often the debounce loop wakes. Small enough to be invisible, large enough not to spin.
const TICK: Duration = Duration::from_millis(50);

/// How long the tree must be quiet before the burst is reported.
///
/// Generous on purpose. The events this exists for arrive in floods — an install, a checkout, a
/// build — and reporting the first quiet moment inside a flood means reloading the tree several
/// times while it is still changing.
const DEBOUNCE: Duration = Duration::from_millis(600);

/// Upper bound on paths carried in one event. Past this the frontend reloads wholesale anyway.
const MAX_PATHS: usize = 200;

/// Directory names never watched and never reported.
///
/// Generated output and tool bookkeeping. Two reasons, and the second is the one that decides it:
/// nothing in them is a file somebody is editing, and they are where the volume is — a `cargo
/// build` writing into `target/` would otherwise be a continuous burst that never goes quiet, so
/// the debounce would never flush and the tree would freeze exactly while the machine is busy.
const SKIP: &[&str] = &[
    "target", "node_modules", ".git", ".svn", ".hg", ".gradle", ".idea", ".vscode",
    "__pycache__", ".venv", ".mypy_cache", ".pytest_cache", ".next", ".svelte-kit",
    ".turbo", ".nuxt", "coverage", ".arbor",
];

/// The running watchers, keyed by root. A workspace has several.
static CURRENT: LazyLock<Mutex<Option<Running>>> = LazyLock::new(|| Mutex::new(None));

/// One watcher thread and the flag that asks it to stop.
struct Running {
    roots: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
}

/// Watch `roots`, replacing whatever was being watched.
///
/// A no-op when the same set is already watched, so re-opening a project does not stack threads.
/// An error means "no live updates" and never a failed open: a tree that has to be refreshed by
/// hand is a smaller problem than a project that will not open.
pub fn watch(sink: Arc<dyn EventSink>, roots: Vec<PathBuf>) -> Result<(), String> {
    let mut slot = CURRENT.lock().map_err(|_| "the watcher slot is poisoned".to_string())?;
    if slot.as_ref().is_some_and(|r| r.roots == roots) {
        return Ok(());
    }
    if let Some(previous) = slot.take() {
        previous.stop.store(true, Ordering::Relaxed);
    }
    if roots.is_empty() {
        return Ok(());
    }

    let stop = Arc::new(AtomicBool::new(false));
    spawn_thread(sink, roots.clone(), Arc::clone(&stop))?;
    *slot = Some(Running { roots, stop });
    Ok(())
}

/// Ask the running watcher to stop. Safe when none is.
pub fn stop() {
    if let Ok(mut slot) = CURRENT.lock() {
        if let Some(running) = slot.take() {
            running.stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Build the watcher and run the debounce loop on a dedicated OS thread.
fn spawn_thread(
    sink: Arc<dyn EventSink>,
    roots: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    // Unbounded on purpose: the channel is drained every TICK, and a dropped event is a change
    // the tree never hears about.
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        // The receiver is gone once the loop ends; a send failure is how this callback learns so.
        let _ = tx.send(res);
    })
    .map_err(|e| format!("watcher: {e}"))?;

    for root in &roots {
        add_root(&mut watcher, root);
    }

    std::thread::Builder::new()
        .name("bennu-tree-watch".to_string())
        .spawn(move || {
            let mut watcher = watcher;
            let mut pending: BTreeSet<(usize, String)> = BTreeSet::new();
            let mut last_change: Option<Instant> = None;

            while !stop.load(Ordering::Relaxed) {
                if let Ok(Ok(event)) = rx.recv_timeout(TICK) {
                    for path in event.paths {
                        let Some((idx, rel)) = interesting(&roots, &path) else { continue };
                        // A directory that appeared directly under a root is not yet watched —
                        // nothing under it would ever be reported. Add it now.
                        if path.is_dir() && path.parent() == Some(roots[idx].as_path()) {
                            let _ = watcher.watch(&path, RecursiveMode::Recursive);
                        }
                        pending.insert((idx, rel));
                    }
                    if !pending.is_empty() {
                        last_change = Some(Instant::now());
                    }
                }
                if last_change.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
                    flush(&sink, &roots, &mut pending);
                    last_change = None;
                }
            }
            // Whatever was pending at close is worth one last event.
            flush(&sink, &roots, &mut pending);
        })
        .map_err(|e| format!("watcher thread: {e}"))?;
    Ok(())
}

/// Watch one root: the directory itself shallowly, and each interesting child recursively.
///
/// Shallow on the root so a *new* top-level directory is seen appearing; recursive on the
/// children so everything inside them is. The point of the split is what is left out — see
/// [`SKIP`] and the module docs.
///
/// Failures are silent per path: a directory that cannot be watched (a permission, a broken
/// symlink, a limit) costs live updates under it and nothing else, and there is nothing the user
/// could do about it from here.
fn add_root(watcher: &mut notify::RecommendedWatcher, root: &Path) {
    let _ = watcher.watch(root, RecursiveMode::NonRecursive);
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if skipped(path.file_name().and_then(|n| n.to_str()).unwrap_or_default()) {
            continue;
        }
        let _ = watcher.watch(&path, RecursiveMode::Recursive);
    }
}

/// Whether a directory name is one never watched.
fn skipped(name: &str) -> bool {
    SKIP.contains(&name)
}

/// Which root a changed path belongs to and its path relative to it — or `None` when the change is
/// outside every root, or inside a directory nobody is editing.
///
/// The **longest** matching root wins, so a workspace member opened in its own right is reported
/// as itself rather than as a path inside the outer project.
fn interesting(roots: &[PathBuf], path: &Path) -> Option<(usize, String)> {
    let mut best: Option<(usize, &Path)> = None;
    for (i, root) in roots.iter().enumerate() {
        if path.starts_with(root)
            && best.map(|(_, b)| root.as_path().as_os_str().len() > b.as_os_str().len()).unwrap_or(true)
        {
            best = Some((i, root.as_path()));
        }
    }
    let (idx, root) = best?;
    let rel = path.strip_prefix(root).ok()?;
    // A `target/` deep inside a crate is still `target/` — the watch above only skips the ones at
    // the top level, and a workspace member's own build directory is not at the top level.
    if rel.components().any(|c| matches!(c, Component::Normal(n) if skipped(&n.to_string_lossy()))) {
        return None;
    }
    Some((idx, rel.to_string_lossy().replace('\\', "/")))
}

/// Emit the accumulated paths, one event per root, and clear them.
fn flush(sink: &Arc<dyn EventSink>, roots: &[PathBuf], pending: &mut BTreeSet<(usize, String)>) {
    if pending.is_empty() {
        return;
    }
    // One event per root and not one carrying all of them: the frontend reloads a tree per root,
    // and a payload it has to demultiplex is a payload that will be demultiplexed wrongly once.
    for (idx, root) in roots.iter().enumerate() {
        let paths: Vec<String> =
            pending.iter().filter(|(i, _)| *i == idx).map(|(_, p)| p.clone()).collect();
        if paths.is_empty() {
            continue;
        }
        let truncated = paths.len() > MAX_PATHS;
        sink.emit(
            TOPIC,
            json!({
                "root": root.to_string_lossy(),
                "paths": paths.into_iter().take(MAX_PATHS).collect::<Vec<_>>(),
                "truncated": truncated,
            }),
        );
    }
    pending.clear();
}

/// Args for [`bennu_watch_roots`].
#[derive(serde::Deserialize)]
pub struct WatchArgs {
    /// Every project root the window has open. An empty list stops watching.
    pub roots: Vec<String>,
}

/// Watch these roots for changes, replacing whatever was watched before.
///
/// Called by the frontend whenever the set of open projects changes, rather than from
/// `bennu_open_project`: a workspace is opened one root at a time and the watcher wants the whole
/// set, so driving it from the opens would mean restarting the thread once per member.
///
/// Never fails the caller. A watcher that cannot start means the tree has to be refreshed by hand,
/// which is a smaller problem than an open that reports an error for something nobody asked for —
/// the reason is on stderr for the one case where somebody is looking.
#[arbor_rpc::handler]
fn bennu_watch_roots(ctx: &BennuState, args: WatchArgs) -> Result<bool, String> {
    let roots: Vec<PathBuf> = args.roots.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    match watch(ctx.event_sink(), roots) {
        Ok(()) => Ok(true),
        Err(e) => {
            eprintln!("bennu-be: tree watcher: {e}");
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_directory_is_never_reported_at_any_depth() {
        let roots = vec![PathBuf::from("/w/app")];
        // The ones the watch already skips at the top level…
        assert!(interesting(&roots, Path::new("/w/app/target/debug/x.rlib")).is_none());
        assert!(interesting(&roots, Path::new("/w/app/node_modules/svelte/index.js")).is_none());
        // …and the ones it cannot, because they are nested inside a member.
        assert!(interesting(&roots, Path::new("/w/app/crates/core/target/debug/x")).is_none());
        assert!(interesting(&roots, Path::new("/w/app/.git/index")).is_none());
        // A file whose NAME merely starts the same is not inside one.
        assert_eq!(
            interesting(&roots, Path::new("/w/app/targets.txt")),
            Some((0, "targets.txt".to_string())),
        );
    }

    #[test]
    fn a_change_is_attributed_to_the_most_specific_root() {
        // A workspace member opened in its own right answers for itself: reporting it as a path
        // inside the outer project would reload the wrong tree.
        let roots = vec![PathBuf::from("/w"), PathBuf::from("/w/member")];
        assert_eq!(
            interesting(&roots, Path::new("/w/member/src/lib.rs")),
            Some((1, "src/lib.rs".to_string())),
        );
        assert_eq!(
            interesting(&roots, Path::new("/w/other/src/lib.rs")),
            Some((0, "other/src/lib.rs".to_string())),
        );
        // Outside every root.
        assert!(interesting(&roots, Path::new("/elsewhere/x.rs")).is_none());
    }
}
