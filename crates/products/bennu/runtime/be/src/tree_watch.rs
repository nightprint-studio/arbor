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
//! - **Debounced, with a ceiling.** Changes accumulate until the tree has been quiet, then leave as
//!   one event — a `git checkout` touching four hundred files is one reload. Only a change that is
//!   *reported* restarts the quiet period, and a burst is never held longer than [`MAX_WAIT`]: a
//!   build writing into a nested `target/` used to restart it forever, so the folder somebody had
//!   just created never showed up while the machine was busy.
//! - **Structural changes only** — see [`crate::tree_changes`]. A file's bytes are not the tree's
//!   business, except for the manifests and `.gitignore`.
//! - **`target/` and `node_modules/` are never watched at all** — not filtered afterwards,
//!   *unwatched*. In this repository those two hold 17 000 of the 17 500 directories, and on Linux
//!   a recursive watch is one inotify handle per directory against a limit that is regularly 8 192.
//!   Watching the root shallowly and each interesting top-level child recursively costs a few
//!   dozen handles instead. Such a directory *appearing* at the top level is still reported: the
//!   tree lists `.idea` and `.vscode`, so their arrival is a change to what it shows.
//! - **Top-level watches follow the listing.** A directory created later gets its recursive watch;
//!   one removed or renamed has its watch dropped. On Windows a watch holds a handle to its
//!   directory, so a renamed one kept reporting under its old name and a deleted one lingered as
//!   delete-pending until the handle closed.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use bennu_project::prelude::TREE_SKIP_DIRS;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::tree_changes::{content_followed, expand, Burst, Raw, RawChange};

/// Topic the frontend listens on. Payload: see [`Burst::into_payload`].
pub const TOPIC: &str = "arbor://bennu/tree-changed";

/// How often the debounce loop wakes. Small enough to be invisible, large enough not to spin.
const TICK: Duration = Duration::from_millis(50);

/// How long the tree must be quiet before the burst is reported.
///
/// Generous on purpose. The events this exists for arrive in floods — an install, a checkout, a
/// build — and reporting the first quiet moment inside a flood means reloading the tree several
/// times while it is still changing.
const DEBOUNCE: Duration = Duration::from_millis(600);

/// The longest a burst is held, quiet or not. A tree that is a few seconds behind a flood is fine;
/// one that waits for the flood to end can wait forever.
const MAX_WAIT: Duration = Duration::from_secs(3);

/// Directory names never watched, and nothing inside them reported.
///
/// Generated output and tool bookkeeping. Two reasons, and the second is the one that decides it:
/// nothing in them is a file somebody is editing, and they are where the volume is.
const SKIP: &[&str] = &[
    "target", "node_modules", ".git", ".svn", ".hg", ".gradle", ".idea", ".vscode",
    "__pycache__", ".venv", ".mypy_cache", ".pytest_cache", ".next", ".svelte-kit",
    ".turbo", ".nuxt", "coverage", ".arbor",
];

/// The running watcher. A workspace has several roots; one thread watches them all.
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

/// The roots being watched right now — empty when nothing is.
fn watched_roots() -> Vec<PathBuf> {
    CURRENT
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|running| running.roots.clone()))
        .unwrap_or_default()
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

    let top_dirs = roots.iter().map(|root| add_root(&mut watcher, root)).collect();
    let pending = roots.iter().map(|_| Burst::default()).collect();
    let mut state = TreeWatch { watcher, roots, top_dirs, pending };

    std::thread::Builder::new()
        .name("bennu-tree-watch".to_string())
        .spawn(move || {
            let mut first: Option<Instant> = None;
            let mut last: Option<Instant> = None;

            while !stop.load(Ordering::Relaxed) {
                let touched = match rx.recv_timeout(TICK) {
                    Ok(Ok(event)) => state.absorb(event),
                    // A watcher error is events lost with nobody saying which.
                    Ok(Err(e)) => {
                        eprintln!("bennu-be: tree watcher: {e}");
                        state.rescan_all();
                        true
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                if touched {
                    let now = Instant::now();
                    first.get_or_insert(now);
                    last = Some(now);
                }
                let quiet = last.is_some_and(|t| t.elapsed() >= DEBOUNCE);
                let overdue = first.is_some_and(|t| t.elapsed() >= MAX_WAIT);
                if quiet || overdue {
                    state.flush(&sink);
                    first = None;
                    last = None;
                }
            }
            // Whatever was pending at close is worth one last event.
            state.flush(&sink);
        })
        .map_err(|e| format!("watcher thread: {e}"))?;
    Ok(())
}

/// Everything the watcher thread owns.
struct TreeWatch {
    watcher: RecommendedWatcher,
    roots: Vec<PathBuf>,
    /// Per root, the top-level directories holding a recursive watch.
    top_dirs: Vec<BTreeSet<String>>,
    /// Per root, what has changed since the last flush.
    pending: Vec<Burst>,
}

impl TreeWatch {
    /// Record what one event says. `true` when anything worth reporting was recorded — the only
    /// thing allowed to restart the quiet period.
    fn absorb(&mut self, event: notify::Event) -> bool {
        if event.need_rescan() {
            self.rescan_all();
            return true;
        }
        let mut touched = false;
        for (raw, path) in expand(&event.kind, &event.paths, Path::exists) {
            let structural = raw != Raw::Content;
            let Some((idx, rel)) = interesting(&self.roots, &path, structural) else { continue };
            if !structural {
                if content_followed(&rel) {
                    self.pending[idx].record_content(rel);
                    touched = true;
                }
                continue;
            }
            let top_level = !rel.contains('/');
            let is_dir = match raw {
                Raw::Created | Raw::RenamedTo => path.is_dir(),
                _ => top_level && self.top_dirs[idx].contains(&rel),
            };
            if top_level && is_dir {
                self.rearm(idx, &path, &rel, raw);
            }
            self.pending[idx].record(RawChange { kind: raw, rel, is_dir });
            touched = true;
        }
        touched
    }

    /// Keep the top-level recursive watches in step with the root's listing — see the module docs.
    ///
    /// A watch that is dropped marks the burst for a rescan: until the drop, the stale handle was
    /// reporting paths under a name that no longer exists.
    fn rearm(&mut self, idx: usize, path: &Path, rel: &str, raw: Raw) {
        match raw {
            Raw::Created | Raw::RenamedTo => {
                if !skipped(rel) && self.top_dirs[idx].insert(rel.to_string()) {
                    let _ = self.watcher.watch(path, RecursiveMode::Recursive);
                }
            }
            Raw::Removed | Raw::RenamedFrom => {
                if self.top_dirs[idx].remove(rel) {
                    let _ = self.watcher.unwatch(path);
                    self.pending[idx].mark_rescan();
                }
            }
            Raw::Content => {}
        }
    }

    /// Every root may have lost events.
    fn rescan_all(&mut self) {
        for burst in &mut self.pending {
            burst.mark_rescan();
        }
    }

    /// Emit the accumulated changes, one event per root, and clear them.
    ///
    /// One event per root and not one carrying all of them: the frontend reloads a tree per root,
    /// and a payload it has to demultiplex is a payload that will be demultiplexed wrongly once.
    fn flush(&mut self, sink: &Arc<dyn EventSink>) {
        for (idx, root) in self.roots.iter().enumerate() {
            let burst = std::mem::take(&mut self.pending[idx]);
            if burst.is_empty() {
                continue;
            }
            let is_module = |rel: &str| root.join(rel).join("pom.xml").is_file();
            sink.emit(TOPIC, burst.into_payload(&root.to_string_lossy(), is_module));
        }
    }
}

/// Watch one root: the directory itself shallowly, and each interesting child recursively.
/// Returns the children that got a recursive watch.
///
/// Shallow on the root so a *new* top-level directory is seen appearing; recursive on the
/// children so everything inside them is. The point of the split is what is left out — see
/// [`SKIP`] and the module docs.
///
/// Failures are silent per path: a directory that cannot be watched (a permission, a broken
/// symlink, a limit) costs live updates under it and nothing else, and there is nothing the user
/// could do about it from here.
fn add_root(watcher: &mut RecommendedWatcher, root: &Path) -> BTreeSet<String> {
    let mut watched = BTreeSet::new();
    let _ = watcher.watch(root, RecursiveMode::NonRecursive);
    let Ok(entries) = std::fs::read_dir(root) else { return watched };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        if watcher.watch(&path, RecursiveMode::Recursive).is_ok() {
            watched.insert(name);
        }
    }
    watched
}

/// Whether a directory name is one never watched.
fn skipped(name: &str) -> bool {
    SKIP.contains(&name)
}

/// Which root a changed path belongs to and its path relative to it — or `None` when the change is
/// outside every root, inside a directory nobody is editing, or about the root itself.
///
/// The **longest** matching root wins, so a workspace member opened in its own right is reported
/// as itself rather than as a path inside the outer project.
///
/// A skipped name as the *last* component is the entry itself, not something inside it: its
/// contents are nobody's business, but a `structural` change to it — `.idea` appearing, `coverage`
/// being deleted — changes a listing the tree shows. The exception is what the tree never lists.
fn interesting(roots: &[PathBuf], path: &Path, structural: bool) -> Option<(usize, String)> {
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
    let names: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let (last, parents) = names.split_last()?;
    // A `target/` deep inside a crate is still `target/` — the watch only skips the ones at the top
    // level, and a workspace member's own build directory is not at the top level.
    if parents.iter().any(|n| skipped(n)) {
        return None;
    }
    if skipped(last) && (!structural || TREE_SKIP_DIRS.contains(&last.as_str())) {
        return None;
    }
    Some((idx, names.join("/")))
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
    // The set this replaces is also the only record of which projects the window just closed:
    // whatever was watched and is not any more. Read before `watch` swaps it out.
    let previous = watched_roots();
    let watched = match watch(ctx.event_sink(), roots.clone()) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("bennu-be: tree watcher: {e}");
            false
        }
    };
    // Whether or not the watcher started: a project closed is closed, and its memory is owed back
    // either way. Released off this thread — see `project_close`.
    crate::project_close::release_closed(&previous, &roots);
    Ok(watched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_directory_is_never_reported_at_any_depth() {
        let roots = vec![PathBuf::from("/w/app")];
        // The ones the watch already skips at the top level…
        assert!(interesting(&roots, Path::new("/w/app/target/debug/x.rlib"), true).is_none());
        assert!(interesting(&roots, Path::new("/w/app/node_modules/svelte/index.js"), true).is_none());
        // …and the ones it cannot, because they are nested inside a member.
        assert!(interesting(&roots, Path::new("/w/app/crates/core/target/debug/x"), true).is_none());
        assert!(interesting(&roots, Path::new("/w/app/.git/index"), true).is_none());
        // A file whose NAME merely starts the same is not inside one.
        assert_eq!(
            interesting(&roots, Path::new("/w/app/targets.txt"), true),
            Some((0, "targets.txt".to_string())),
        );
    }

    #[test]
    fn a_skipped_directory_appearing_is_a_change_to_the_listing_it_sits_in() {
        let roots = vec![PathBuf::from("/w/app")];
        // The tree lists `.idea`: its arrival is news, its contents are not.
        assert_eq!(interesting(&roots, Path::new("/w/app/.idea"), true), Some((0, ".idea".to_string())));
        assert!(interesting(&roots, Path::new("/w/app/.idea"), false).is_none());
        assert!(interesting(&roots, Path::new("/w/app/.idea/workspace.xml"), true).is_none());
        // The tree never lists `target`, so neither is its arrival.
        assert!(interesting(&roots, Path::new("/w/app/core/target"), true).is_none());
        // The root itself is not an entry of any listing.
        assert!(interesting(&roots, Path::new("/w/app"), true).is_none());
    }

    #[test]
    fn a_change_is_attributed_to_the_most_specific_root() {
        // A workspace member opened in its own right answers for itself: reporting it as a path
        // inside the outer project would reload the wrong tree.
        let roots = vec![PathBuf::from("/w"), PathBuf::from("/w/member")];
        assert_eq!(
            interesting(&roots, Path::new("/w/member/src/lib.rs"), true),
            Some((1, "src/lib.rs".to_string())),
        );
        assert_eq!(
            interesting(&roots, Path::new("/w/other/src/lib.rs"), true),
            Some((0, "other/src/lib.rs".to_string())),
        );
        // Outside every root.
        assert!(interesting(&roots, Path::new("/elsewhere/x.rs"), true).is_none());
    }
}
