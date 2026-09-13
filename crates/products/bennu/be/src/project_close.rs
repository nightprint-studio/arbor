//! Closing a project: giving back what opening it took.
//!
//! ## Why this exists
//!
//! Opening a project fills half a dozen process-wide caches — the symbol index slot (sources as
//! text, a reference index with a preview line per use site, the decoded classpath, the mapped
//! index files), the framework models, the library index, the entry points — and starts language
//! servers. Until this module, **nothing ever emptied any of them**. The slot maps were only ever
//! inserted into, so every project opened during a session stayed loaded until the backend
//! exited: moving between a few large projects held all of them, which is how `bennu-be` came to
//! report more than a gigabyte while showing one.
//!
//! ## The signal: the set of watched roots, not a new "close" call
//!
//! The window already tells the backend which projects it has open — `bennu_watch_roots`, sent
//! with the **whole set** every time it changes: a member removed, the workspace replaced by
//! opening a different project, or another workspace switched to. A root that was in the previous
//! set and is not in the new one has left. That rule has a property a close call would not: a
//! project an AI client opened through the agent surface never appears in the window's set, so the
//! window closing its own projects can never take one away from the client.
//!
//! Switching between the projects of the workspace you are in does not change the set, so it
//! releases nothing — those stay loaded, and switching between them stays instant.
//!
//! ## Released after a settle, not at once
//!
//! The watched set passes through states that are not closes. Switching workspace empties it and
//! refills it one project at a time, each behind an `await`; read literally, a window reporting its
//! roots mid-load would close every project it is about to open again — a full re-index of each,
//! and a release racing the reopen of the very slot it is removing. So a root that leaves the set is
//! only **scheduled**: it is released if it is still gone [`SETTLE`] later, and reappearing in the
//! set, or being opened again by any door, cancels it.
//!
//! ## The registry: the key an open used
//!
//! Every engine keys a project by the string it was opened with. The window, though, names a
//! project by the root the backend *resolved* — which is not always the string that was sent (a
//! folder picked below a manifest, a trailing slash). Releasing by the watched name would then look
//! up a key nobody used and free nothing, without saying so. So each open records its exact key
//! under both names, and a release goes through the record.
//!
//! ## And a project nobody is looking at
//!
//! The watched set says which projects the window *holds*, which is every member of the workspace
//! — and holding them all loaded is how switching between five projects came to keep five indexes,
//! five sets of framework models and a rust-analyzer apiece for as long as the session lasted. So a
//! second rule sits beside the first: a project that has gone
//! [`inactive_project_release_secs`](bennu_core::prelude::LspConfig) without being **on screen** or
//! **asked about** is released the same way, whether the window still lists it or an AI client
//! opened it and moved on.
//!
//! "Asked about" is recorded where every question passes — the index's slot lookup, the framework
//! host's, the language-server lease — so it holds for the window's cross-project searches and for
//! a client's tool calls alike, with no door having to remember to say so. The project on screen is
//! never released, however long it sits untouched: leaving it is what starts its clock. Coming back
//! to a released one opens it again through the same door that opened it the first time.
//!
//! ## What is not released
//!
//! Nothing on disk is touched: index generations and the include cache stay where they are. And
//! freed is not the same as returned — the allocator keeps small blocks for reuse, so resident
//! memory falls by less than was freed. What matters is that it stops growing with every project
//! opened.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::frameworks::FrameworkService;
use crate::index_service::IndexService;
use crate::lsp_registry::LspRegistry;

/// How long a root must stay out of the watched set before it is released.
///
/// Long enough to cover a workspace switch reopening its members one by one, and a quick switch
/// away and straight back; short enough that closing a project shows in the process monitor while
/// you are still looking at it.
const SETTLE: Duration = Duration::from_secs(20);

/// Normalised project name → every exact key an open used for it.
static OPENED: LazyLock<Mutex<HashMap<String, HashSet<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Roots scheduled for release → the schedule that owns each.
///
/// A release goes ahead only if its own schedule is still the entry: a cancel removes it, and a
/// later schedule for the same root replaces it — so of two overlapping schedules only the newest
/// can release, and only once.
static PENDING: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_SCHEDULE: AtomicU64 = AtomicU64::new(1);

/// Normalised project name → when it was last on screen or asked about.
static TOUCHED: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether the idle reaper is running. Started with the first open, never stopped.
static REAPING: AtomicBool = AtomicBool::new(false);

/// How often the idle reaper looks. It decides *when* an idle project goes, so a minute of slack
/// costs a minute of memory and saves the wake-ups.
const REAP_INTERVAL: Duration = Duration::from_secs(60);

/// One spelling for comparing roots: forward slashes, no trailing slash.
fn norm(root: &str) -> String {
    root.replace('\\', "/").trim_end_matches('/').to_string()
}

/// Remember the key an open used, under the name it was asked for and the name it resolved to —
/// and call off any release scheduled for it: a project opened again is not closed.
///
/// Returns whether this is the **first** open of that key since it was last released — the moment
/// once-per-open work (history retention) belongs to, rather than every activation that re-enters
/// through the same door.
pub(crate) fn opened(requested: &str, resolved: &str) -> bool {
    let first = {
        let mut map = OPENED.lock().unwrap_or_else(|p| p.into_inner());
        let first = map.entry(norm(requested)).or_default().insert(requested.to_string());
        map.entry(norm(resolved)).or_default().insert(requested.to_string());
        first
    };
    cancel([norm(requested), norm(resolved)]);
    touch(requested);
    touch(resolved);
    start_reaper();
    first
}

/// Record that `root` is in use — on screen, or asked a question. See the module doc.
pub(crate) fn touch(root: &str) {
    TOUCHED.lock().unwrap_or_else(|p| p.into_inner()).insert(norm(root), Instant::now());
}

/// Release, on its own thread and forever, every project idle beyond the configured time.
fn start_reaper() {
    if REAPING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("bennu-project-reaper".to_string())
        .spawn(|| loop {
            std::thread::sleep(REAP_INTERVAL);
            release_idle();
        });
    if spawned.is_err() {
        // Not fatal and not silent: projects are still released when they leave the workspace.
        eprintln!("bennu-be: could not spawn the project reaper; idle projects will stay loaded");
        REAPING.store(false, Ordering::SeqCst);
    }
}

/// One pass of the reaper. The setting is read each time, so changing it applies without a restart;
/// `0` keeps every project loaded.
fn release_idle() {
    let timeout = bennu_core::prelude::load_config().lsp.inactive_project_release_secs;
    if timeout == 0 {
        return;
    }
    let opened = OPENED.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let touched = TOUCHED.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let active = crate::project::active_root().map(|r| norm(&r));
    let idle = idle_projects(&opened, &touched, active.as_deref(), Instant::now(), Duration::from_secs(timeout));
    for root in idle {
        eprintln!("bennu-be: releasing {root} — idle for more than {timeout}s");
        release(&root);
    }
}

/// The projects to release: one name per opened key, when **every** name that key is known by has
/// been idle for `timeout` and none of them is the project on screen.
///
/// Every name, because a project is recorded under the name it was asked for and the one it
/// resolved to, and a question may arrive under either: judging one name alone would release a
/// project that was asked about a second ago under its other spelling. A name never touched counts
/// as idle — it cannot happen through [`opened`], which touches both.
fn idle_projects(
    opened: &HashMap<String, HashSet<String>>,
    touched: &HashMap<String, Instant>,
    active: Option<&str>,
    now: Instant,
    timeout: Duration,
) -> Vec<String> {
    let mut names_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for (name, keys) in opened {
        for key in keys {
            names_of.entry(key.as_str()).or_default().push(name.as_str());
        }
    }
    let idle = |name: &str| {
        Some(name) != active
            && touched.get(name).is_none_or(|at| now.saturating_duration_since(*at) >= timeout)
    };
    let mut out: Vec<String> = names_of
        .values()
        .filter(|names| names.iter().all(|n| idle(n)))
        .filter_map(|names| names.iter().min().map(|n| n.to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Schedule the release of every root watched in `previous` and absent from `current`, and cancel
/// any scheduled release for a root that is in `current`.
pub(crate) fn release_closed(previous: &[PathBuf], current: &[PathBuf]) {
    cancel(current.iter().map(|p| norm(&p.to_string_lossy())));
    let closed = closed_roots(previous, current);
    if closed.is_empty() {
        return;
    }
    let schedule = schedule(&closed);
    // Waiting and releasing both happen here, off the request: the settle is a sleep, and dropping
    // the slot of a large project is seconds of deallocation.
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE);
        for root in closed {
            if still_due(&root, schedule) {
                release(&root);
            }
        }
    });
}

/// The roots in `previous` that are not in `current`, normalised.
fn closed_roots(previous: &[PathBuf], current: &[PathBuf]) -> Vec<String> {
    let current: HashSet<String> = current.iter().map(|p| norm(&p.to_string_lossy())).collect();
    let mut seen = HashSet::new();
    previous
        .iter()
        .map(|p| norm(&p.to_string_lossy()))
        .filter(|root| !current.contains(root) && seen.insert(root.clone()))
        .collect()
}

fn schedule(roots: &[String]) -> u64 {
    let id = NEXT_SCHEDULE.fetch_add(1, Ordering::Relaxed);
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    for root in roots {
        pending.insert(root.clone(), id);
    }
    id
}

fn cancel(roots: impl IntoIterator<Item = String>) {
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    for root in roots {
        pending.remove(&root);
    }
}

/// Whether `schedule` still owns `root` — taking the entry if so, so it cannot be released twice.
fn still_due(root: &str, schedule: u64) -> bool {
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    if pending.get(root) == Some(&schedule) {
        pending.remove(root);
        true
    } else {
        false
    }
}

/// Every exact key recorded for `root`, removed from the record along with any other name that
/// pointed at the same keys. `root` itself is always included, so a project the record never saw
/// is still released under its own name.
fn take_keys(root: &str) -> HashSet<String> {
    let mut map = OPENED.lock().unwrap_or_else(|p| p.into_inner());
    let mut keys = map.remove(root).unwrap_or_default();
    keys.insert(root.to_string());
    map.retain(|_, recorded| {
        recorded.retain(|key| !keys.contains(key));
        !recorded.is_empty()
    });
    keys
}

/// Give back everything the engines hold for `root`.
///
/// One window remains, and it is narrow: a reopen landing between the settle check and the end of
/// this function can have its fresh slot removed. The effect is a project that reads as not indexed
/// until its next open or rebuild — recoverable, and far rarer than the releases the settle exists
/// to prevent.
fn release(root: &str) {
    let keys = take_keys(root);
    {
        let mut touched = TOUCHED.lock().unwrap_or_else(|p| p.into_inner());
        touched.remove(root);
        for key in &keys {
            touched.remove(&norm(key));
        }
    }
    for key in keys {
        // Servers first: they are separate processes, and stopping them frees memory outright
        // rather than into the allocator.
        let servers = LspRegistry::global().stop_root(&key);
        IndexService::global().forget(&key);
        FrameworkService::global().forget(&key);
        crate::main_classes::forget_main_classes(&key);
        crate::library_beans::forget(&key);
        crate::library_search::forget(&key);
        crate::wgsl_library::forget(Path::new(&key));
        crate::cargo_intel::forget_under(&key);
        eprintln!("bennu-be: released {key} — {servers} language server(s) stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The statics are shared by tests running in parallel, so each test works on roots of its own.

    #[test]
    fn a_closed_root_is_one_that_was_watched_and_no_longer_is() {
        let before = vec![PathBuf::from("/t1/a"), PathBuf::from("/t1/b/")];
        let after = vec![PathBuf::from("/t1/b")];
        // A trailing slash is not a different project.
        assert_eq!(closed_roots(&before, &after), vec!["/t1/a".to_string()]);
    }

    #[test]
    fn an_emptied_set_would_close_everything_which_is_why_nothing_is_released_at_once() {
        // What a workspace switch reports mid-load. The schedule is the whole defence: a root that
        // comes straight back is cancelled before its settle ends.
        let before = vec![PathBuf::from("/t2/a"), PathBuf::from("/t2/b")];
        let closed = closed_roots(&before, &[]);
        assert_eq!(closed.len(), 2);
        let id = schedule(&closed);
        cancel(["/t2/a".to_string()]);
        assert!(!still_due("/t2/a", id), "reappeared, so not released");
        assert!(still_due("/t2/b", id), "still gone, so released");
    }

    #[test]
    fn only_the_newest_schedule_can_release_and_only_once() {
        let first = schedule(&["/t3/a".to_string()]);
        let second = schedule(&["/t3/a".to_string()]);
        assert!(!still_due("/t3/a", first));
        assert!(still_due("/t3/a", second));
        assert!(!still_due("/t3/a", second), "taken on the way out");
    }

    #[test]
    fn opening_a_project_again_calls_off_its_release() {
        let id = schedule(&["/t4/a".to_string()]);
        opened("/t4/a", "/t4/a");
        assert!(!still_due("/t4/a", id));
    }

    fn opened_map(entries: &[(&str, &[&str])]) -> HashMap<String, HashSet<String>> {
        entries
            .iter()
            .map(|(name, keys)| (name.to_string(), keys.iter().map(|k| k.to_string()).collect()))
            .collect()
    }

    #[test]
    fn a_project_idle_past_the_timeout_is_released_and_a_recent_one_is_not() {
        let now = Instant::now() + Duration::from_secs(10_000);
        let opened = opened_map(&[("/i1/old", &["/i1/old"]), ("/i1/new", &["/i1/new"])]);
        let touched: HashMap<String, Instant> = [
            ("/i1/old".to_string(), now - Duration::from_secs(700)),
            ("/i1/new".to_string(), now - Duration::from_secs(5)),
        ]
        .into();
        let idle = idle_projects(&opened, &touched, None, now, Duration::from_secs(600));
        assert_eq!(idle, vec!["/i1/old".to_string()]);
    }

    #[test]
    fn the_project_on_screen_is_never_released_however_long_it_sits() {
        let now = Instant::now() + Duration::from_secs(10_000);
        let opened = opened_map(&[("/i2/shown", &["/i2/shown"])]);
        let touched: HashMap<String, Instant> = [("/i2/shown".to_string(), now - Duration::from_secs(9_000))].into();
        assert!(idle_projects(&opened, &touched, Some("/i2/shown"), now, Duration::from_secs(600)).is_empty());
    }

    #[test]
    fn a_question_under_either_name_keeps_the_project() {
        // Opened by the folder picked, and asked about by the root it resolved to a second ago.
        let now = Instant::now() + Duration::from_secs(10_000);
        let opened = opened_map(&[("/i3/picked", &["/i3/picked/"]), ("/i3/resolved", &["/i3/picked/"])]);
        let touched: HashMap<String, Instant> = [
            ("/i3/picked".to_string(), now - Duration::from_secs(900)),
            ("/i3/resolved".to_string(), now - Duration::from_secs(1)),
        ]
        .into();
        assert!(idle_projects(&opened, &touched, None, now, Duration::from_secs(600)).is_empty());
    }

    #[test]
    fn only_the_first_open_of_a_key_says_so() {
        assert!(opened("/t6/a", "/t6/a"));
        assert!(!opened("/t6/a", "/t6/a"), "an activation re-entering the door is not a first open");
        take_keys("/t6/a");
        assert!(opened("/t6/a", "/t6/a"), "after a release it is a first open again");
    }

    #[test]
    fn a_release_finds_the_key_the_open_used_under_either_name() {
        // Opened by the folder the user picked; the window names it by the root that resolved.
        opened("/t5/picked/", "/t5/resolved");
        let keys = take_keys("/t5/resolved");
        assert!(keys.contains("/t5/picked/"), "the exact key the engines were keyed by");
        // …and the other name no longer points at it.
        assert!(!take_keys("/t5/picked").contains("/t5/picked/"));
    }
}
