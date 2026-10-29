//! `build_queue` — the broker between everyone who wants to build, test or check a project.
//!
//! The user's Build button, a test run, a whole-project validation and every conversation of the assistant
//! all end up here before a compiler starts. The rules (numbered tickets, fair FIFO per target,
//! coalescing, cancel, timeouts) are `bennu_live::queue`; this is the process-wide instance and what is
//! specific to Bennu around it:
//!
//! * **Who is asking.** A tool call of the assistant runs on the assistant's own thread; it installs a
//!   [`CallContext`] (conversation, cancel flag, build isolation) around the handler it calls, so the
//!   handler needs no new argument. A call with no context is the user's.
//! * **Which target.** The project root; with *separate target* isolation on a Cargo project, the root plus
//!   the conversation — its own `CARGO_TARGET_DIR`, so it neither waits for nor blocks the others.
//! * **What the tree was.** [`JOURNAL`] numbers every write of a conversation (an *epoch*); a run records the
//!   epoch it started at, so the result handed to every waiter can say which tree it is about and which
//!   files changed since.
//! * **Telling the panel.** Every change of the queue is an `arbor://bennu/build-queue` event.
//!
//! Waiting blocks the calling thread (never an async worker: handlers run on the serve loop's per-request
//! thread, tools on the assistant's own).

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_assistant::prelude::{fnv64, Inbox, Journal, Writer};
use bennu_core::prelude::BennuState;
use bennu_live::prelude::{Lead, Queue, QueueOwner, QueueRequest, QueueRow, Wait};
use serde_json::{json, Value};

pub(crate) use bennu_live::prelude::QueueKind as Kind;
pub(crate) use bennu_live::prelude::QueueOwner as Owner;

/// Every change of the queue: carries the whole list, so the panel never reconciles deltas.
pub(crate) const EVT_QUEUE: &str = "arbor://bennu/build-queue";

/// How long a request waits for its turn before giving up. A turn is a build or a test run, not an
/// instant, so this is generous; it exists so a wedged run cannot hold callers forever.
const WAIT_LIMIT: Duration = Duration::from_secs(75 * 60);

/// A wait this long is worth saying in the result.
const NOTEWORTHY_WAIT: Duration = Duration::from_secs(2);

static SINK: Mutex<Option<Arc<dyn EventSink>>> = Mutex::new(None);

static QUEUE: LazyLock<Queue> = LazyLock::new(|| {
    let q = Queue::new();
    q.set_observer(announce);
    q
});

/// The record of which conversation wrote which file, shared by the whole process.
static JOURNAL: LazyLock<Journal> = LazyLock::new(Journal::default);

/// The system notes waiting for conversations.
static INBOX: LazyLock<Inbox> = LazyLock::new(Inbox::default);

pub(crate) fn journal() -> &'static Journal {
    &JOURNAL
}

pub(crate) fn inbox() -> &'static Inbox {
    &INBOX
}

/// Remember where to announce the queue. The first sink wins: it is the same transport every time.
fn bind_sink(sink: Arc<dyn EventSink>) {
    let mut slot = SINK.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_none() {
        *slot = Some(sink);
    }
}

fn announce() {
    let sink = SINK.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(sink) = sink {
        sink.emit(EVT_QUEUE, json!({ "rows": rows_json(&QUEUE.snapshot()) }));
    }
}

fn rows_json(rows: &[QueueRow]) -> Value {
    serde_json::to_value(rows).unwrap_or_else(|_| json!([]))
}

// ── Who is asking ────────────────────────────────────────────────────────────────────────────────

/// Whether a conversation builds in the shared target (queued behind the others) or in a target of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Isolation {
    #[default]
    Shared,
    SeparateTarget,
}

impl Isolation {
    pub(crate) fn parse(word: &str) -> Self {
        if word.trim().eq_ignore_ascii_case("separate") { Isolation::SeparateTarget } else { Isolation::Shared }
    }

}

/// What a handler needs to know about the caller that is not in its arguments.
#[derive(Clone)]
pub(crate) struct CallContext {
    pub owner: QueueOwner,
    pub cancel: Arc<AtomicBool>,
    pub isolation: Isolation,
}

thread_local! {
    static CURRENT: RefCell<Option<CallContext>> = const { RefCell::new(None) };
}

/// Run `f` as `context`'s caller. Restores the previous context even if `f` panics.
pub(crate) fn with_context<R>(context: CallContext, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<CallContext>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CURRENT.with(|c| c.borrow_mut().replace(context)));
    f()
}

pub(crate) fn current() -> Option<CallContext> {
    CURRENT.with(|c| c.borrow().clone())
}

fn owner_now() -> QueueOwner {
    current().map(|c| c.owner).unwrap_or_else(QueueOwner::user)
}

fn cancelled_now() -> bool {
    current().is_some_and(|c| c.cancel.load(Ordering::SeqCst))
}

// ── Targets ──────────────────────────────────────────────────────────────────────────────────────

/// The key a root is queued under: one spelling per folder.
pub(crate) fn busy_key(root: &str) -> String {
    let unified = root.replace('\\', "/");
    let trimmed = unified.trim_end_matches('/');
    if cfg!(windows) { trimmed.to_lowercase() } else { trimmed.to_string() }
}

/// Whether the caller builds this root in a target of its own. Only Cargo has a cheap, safe way to do it
/// (an environment variable); every other toolchain shares the project's output and queues.
fn wants_separate_target(root: &str) -> Option<String> {
    let c = current()?;
    (c.isolation == Isolation::SeparateTarget && !c.owner.is_user() && crate::build::is_cargo_root(std::path::Path::new(root))).then(|| c.owner.conversation)
}

/// The queue target of `root` for the current caller.
pub(crate) fn target_key(root: &str) -> String {
    match wants_separate_target(root) {
        Some(conversation) => format!("{}#target:{conversation}", busy_key(root)),
        None => busy_key(root),
    }
}

/// The `CARGO_TARGET_DIR` of a conversation that builds apart: under the app's data directory, one folder
/// per conversation and project. It costs disk and a cold first compile, which is the point of the option.
pub(crate) fn separate_target_dir(root: &str) -> Option<PathBuf> {
    let conversation = wants_separate_target(root)?;
    let project = format!("{:016x}", fnv64(busy_key(root).as_bytes()));
    Some(arbor_core::prelude::bennu_data_dir().join("build-targets").join(conversation).join(project))
}

/// Point a cargo command at the caller's own target directory, when it has one.
pub(crate) fn apply_target_dir(cmd: &mut Command, root: &str) {
    if let Some(dir) = separate_target_dir(root) {
        cmd.env("CARGO_TARGET_DIR", dir);
    }
}

/// Whether the current caller asked for a separate target that this root cannot give it.
pub(crate) fn separate_target_unsupported(root: &str) -> bool {
    current().is_some_and(|c| c.isolation == Isolation::SeparateTarget && !c.owner.is_user()) && wants_separate_target(root).is_none()
}

// ── Taking a turn ────────────────────────────────────────────────────────────────────────────────

fn request(root: &str, kind: Kind, class: String, label: &str, coalesce: bool) -> QueueRequest {
    QueueRequest { target: target_key(root), class, kind, label: label.to_string(), owner: owner_now(), coalesce }
}

fn give_up(outcome: Wait<'_>, what: &str) -> String {
    match outcome {
        Wait::Cancelled => format!("The {what} was cancelled while it waited in the build queue."),
        Wait::TimedOut => format!("Gave up waiting for a turn in the build queue after {} minutes.", WAIT_LIMIT.as_secs() / 60),
        _ => format!("The {what} could not be queued."),
    }
}

/// The right to use a project's tree: held for as long as the build, test run or check lasts and released on
/// drop, so an early return or a panic cannot leave the target blocked.
pub(crate) struct BuildGuard {
    lead: Option<Lead<'static>>,
}

impl BuildGuard {
    /// Wait for this root's turn and take it, on its own — a request that cannot share a run with another
    /// (a validation, a test run the panel streams). Blocks the calling thread while others are ahead.
    pub(crate) fn acquire(sink: Arc<dyn EventSink>, root: &str, kind: Kind, label: &str) -> Result<Self, String> {
        bind_sink(sink);
        let ticket = QUEUE.submit(request(root, kind, format!("exclusive:{label}"), label, false), Instant::now());
        match QUEUE.wait(ticket.id, &cancelled_now, Some(WAIT_LIMIT)) {
            Wait::Lead(lead) => Ok(Self { lead: Some(lead) }),
            other => Err(give_up(other, kind.word())),
        }
    }

    /// A guard for a caller whose turn is already held further up (the coalescing wrapper): it holds nothing.
    pub(crate) fn already_held() -> Self {
        Self { lead: None }
    }
}

impl Drop for BuildGuard {
    fn drop(&mut self) {
        if let Some(lead) = self.lead.take() {
            lead.finish(Ok(Arc::new(())));
        }
    }
}

/// A result, and what is worth knowing about the run that made it.
pub(crate) struct Served<T> {
    pub value: T,
    /// The journal's epoch when the run began: the tree it is about.
    pub epoch: u64,
    /// How many requests the run served, this one included.
    pub served: usize,
    /// Whether another request's run produced this (no run was made for it).
    pub rode: bool,
    pub waited: Duration,
    /// The queue target it ran on, for the green/red bookkeeping.
    pub target: String,
}

struct Stamped<T> {
    value: T,
    epoch: u64,
}

/// Take a ticket for `kind` on `root` and return the result of the run that serves it: its own, or one a
/// request for the same `class` queued before it started. `run` is called on this thread only when it is
/// this ticket's turn to make the run. `is_green` says whether a result is a clean one, for the record of
/// which tree last compiled.
pub(crate) fn coalesced<T: Clone + Send + Sync + 'static>(
    sink: Arc<dyn EventSink>,
    root: &str,
    kind: Kind,
    class: String,
    label: &str,
    is_green: impl Fn(&T) -> bool,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<Served<T>, String> {
    bind_sink(sink);
    let target = target_key(root);
    let started = Instant::now();
    let ticket = QUEUE.submit(request(root, kind, class, label, true), started);
    match QUEUE.wait(ticket.id, &cancelled_now, Some(WAIT_LIMIT)) {
        Wait::Lead(lead) => {
            let waited = started.elapsed();
            let epoch = JOURNAL.epoch();
            let result = run();
            let riders = lead.riders() + 1;
            match &result {
                Ok(value) => {
                    if is_green(value) {
                        JOURNAL.mark_green(&target, epoch);
                        INBOX.forget_seen();
                    }
                    lead.finish(Ok(Arc::new(Stamped { value: value.clone(), epoch })));
                }
                Err(e) => lead.finish(Err(e.clone())),
            }
            result.map(|value| Served { value, epoch, served: riders, rode: false, waited, target })
        }
        Wait::Shared(shared) => {
            let payload = shared.result?;
            let stamped = payload.downcast::<Stamped<T>>().map_err(|_| "the shared run returned something unexpected".to_string())?;
            Ok(Served { value: stamped.value.clone(), epoch: stamped.epoch, served: shared.served, rode: true, waited: shared.waited, target })
        }
        other => Err(give_up(other, kind.word())),
    }
}

// ── Saying what the tree was ─────────────────────────────────────────────────────────────────────

/// A sentence for the model (or the panel) about how a result came about: shared with other requests,
/// waited for, and the files that changed after it started. `None` when it was an ordinary run nobody waited
/// for and nothing changed under.
pub(crate) fn tree_note(kind: Kind, rode: bool, served: usize, waited: Duration, epoch: u64, changed: &[Writer]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if rode {
        parts.push(format!("This {} was made for {served} requests at once; yours was queued before it started, so you received the same result.", kind.word()));
    } else if served > 1 {
        parts.push(format!("This {} also served {} other request(s) that asked for the same thing while it was queued.", kind.word(), served - 1));
    }
    if waited >= NOTEWORTHY_WAIT {
        parts.push(format!("It waited {}s in the build queue.", waited.as_secs()));
    }
    if rode || served > 1 || waited >= NOTEWORTHY_WAIT {
        parts.push(format!("It is about the project as of edit #{epoch}."));
    }
    if !changed.is_empty() {
        let list: Vec<String> = changed
            .iter()
            .take(8)
            .map(|w| {
                let name = w.file.rsplit(['/', '\\']).next().unwrap_or(&w.file);
                if w.title.trim().is_empty() { name.to_string() } else { format!("{name} (\"{}\")", w.title.trim()) }
            })
            .collect();
        let more = if changed.len() > 8 { format!(" and {} more", changed.len() - 8) } else { String::new() };
        parts.push(format!("Files changed since it started: {}{more} — the result may not describe them.", list.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// [`tree_note`] for a result this module served, plus the isolation caveat when the caller asked for a
/// separate target the toolchain cannot give.
pub(crate) fn served_note<T>(kind: Kind, root: &str, served: &Served<T>) -> Option<String> {
    let changed = JOURNAL.changed_since(served.epoch);
    let mut parts: Vec<String> = tree_note(kind, served.rode, served.served, served.waited, served.epoch, &changed).into_iter().collect();
    if separate_target_unsupported(root) {
        parts.push("The conversation asked for a separate build target, which only Cargo projects support: this ran in the project's shared target, queued with the other builds.".to_string());
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

// ── Handlers ─────────────────────────────────────────────────────────────────────────────────────

/// The queue as it is now: the run in progress and the tickets waiting, per project. The panel also hears
/// every change as `arbor://bennu/build-queue`.
#[arbor_rpc::handler]
fn bennu_build_queue(ctx: &BennuState) -> Result<Value, String> {
    bind_sink(ctx.event_sink());
    Ok(json!({ "rows": rows_json(&QUEUE.snapshot()) }))
}

/// Withdraw a waiting ticket. False when it is the run in progress (stop that one with its own Cancel) or is
/// already gone.
#[arbor_rpc::handler]
fn bennu_build_queue_cancel(_ctx: &BennuState, ticket: u64) -> Result<bool, String> {
    Ok(QUEUE.cancel(ticket))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer(file: &str, title: &str) -> Writer {
        Writer { conversation: "c".into(), title: title.into(), epoch: 1, file: file.into() }
    }

    fn context(conversation: &str, isolation: Isolation) -> CallContext {
        CallContext { owner: QueueOwner { conversation: conversation.into(), title: "T".into() }, cancel: Arc::new(AtomicBool::new(false)), isolation }
    }

    #[test]
    fn an_ordinary_solo_run_has_nothing_to_add() {
        assert!(tree_note(Kind::Build, false, 1, Duration::from_millis(10), 5, &[]).is_none());
    }

    #[test]
    fn a_shared_run_says_so_and_names_its_tree() {
        let text = tree_note(Kind::Build, true, 3, Duration::from_secs(40), 12, &[]).unwrap();
        assert!(text.contains("3 requests") && text.contains("same result") && text.contains("edit #12") && text.contains("40s"), "{text}");
    }

    #[test]
    fn files_changed_after_the_start_are_named_with_their_conversation() {
        let text = tree_note(Kind::Test, false, 1, Duration::ZERO, 2, &[writer("/p/src/lib.rs", "Fix cart")]).unwrap();
        assert!(text.contains("lib.rs (\"Fix cart\")") && text.contains("may not describe"), "{text}");
    }

    #[test]
    fn a_long_change_list_is_capped() {
        let many: Vec<Writer> = (0..12).map(|i| writer(&format!("/p/f{i}.rs"), "")).collect();
        let text = tree_note(Kind::Build, false, 1, Duration::ZERO, 1, &many).unwrap();
        assert!(text.contains("and 4 more"), "{text}");
    }

    #[test]
    fn the_context_is_scoped_to_the_closure() {
        assert!(current().is_none());
        with_context(context("c-1", Isolation::Shared), || assert_eq!(current().unwrap().owner.conversation, "c-1"));
        assert!(current().is_none());
        let _ = std::panic::catch_unwind(|| with_context(context("c-2", Isolation::Shared), || panic!("boom")));
        assert!(current().is_none(), "a panic does not leave the context behind");
    }

    #[test]
    fn a_shared_conversation_queues_on_the_project_and_the_user_always_does() {
        with_context(context("c-1", Isolation::Shared), || assert_eq!(target_key("C:\\P\\App"), busy_key("c:/p/app")));
        assert_eq!(target_key("/work/app/"), "/work/app");
    }

    #[test]
    fn the_isolation_word_round_trips_and_defaults_to_shared() {
        assert_eq!(Isolation::parse("separate"), Isolation::SeparateTarget);
        assert_eq!(Isolation::parse(""), Isolation::Shared);
        assert_eq!(Isolation::parse("nonsense"), Isolation::Shared);
    }

    fn sink() -> Arc<dyn EventSink> {
        struct Quiet;
        impl EventSink for Quiet {
            fn emit(&self, _topic: &str, _payload: Value) {}
        }
        Arc::new(Quiet)
    }

    #[test]
    fn two_requests_for_one_class_make_one_run() {
        let root = "/queue-test/coalesce";
        let runs = Arc::new(Mutex::new(0u32));
        let gate = Arc::new(Mutex::new(()));
        // The first request holds the target until the second and third have queued behind it.
        let held = gate.lock().unwrap();
        let first = {
            let (runs, gate) = (Arc::clone(&runs), Arc::clone(&gate));
            std::thread::spawn(move || {
                coalesced(sink(), root, Kind::Build, "a".into(), "check a", |_: &u32| true, || {
                    let _wait = gate.lock().unwrap();
                    *runs.lock().unwrap() += 1;
                    Ok(7u32)
                })
            })
        };
        std::thread::sleep(Duration::from_millis(200));
        let waiters: Vec<_> = (0..2)
            .map(|_| {
                let runs = Arc::clone(&runs);
                std::thread::spawn(move || {
                    coalesced(sink(), root, Kind::Build, "b".into(), "check b", |_: &u32| true, || {
                        *runs.lock().unwrap() += 1;
                        Ok(9u32)
                    })
                })
            })
            .collect();
        std::thread::sleep(Duration::from_millis(200));
        drop(held);
        assert_eq!(first.join().unwrap().unwrap().value, 7);
        let results: Vec<Served<u32>> = waiters.into_iter().map(|h| h.join().unwrap().unwrap()).collect();
        assert!(results.iter().all(|r| r.value == 9), "both got the one result of class b");
        assert_eq!(results.iter().filter(|r| r.rode).count(), 1, "exactly one of them rode the other's run");
        assert_eq!(*runs.lock().unwrap(), 2, "one run for a, one shared run for b");
    }

    #[test]
    fn a_guard_frees_the_target_when_dropped() {
        let root = "/queue-test/guard";
        let first = BuildGuard::acquire(sink(), root, Kind::Check, "validate").unwrap();
        assert!(QUEUE.busy(&busy_key(root)));
        drop(first);
        assert!(!QUEUE.busy(&busy_key(root)));
        drop(BuildGuard::acquire(sink(), root, Kind::Check, "validate").unwrap());
    }

    #[test]
    fn a_cancelled_caller_does_not_wait_forever() {
        let root = "/queue-test/cancel";
        let _holder = BuildGuard::acquire(sink(), root, Kind::Build, "build").unwrap();
        let ctx = context("c-9", Isolation::Shared);
        ctx.cancel.store(true, Ordering::SeqCst);
        let err = with_context(ctx, || BuildGuard::acquire(sink(), root, Kind::Build, "build").err().unwrap());
        assert!(err.contains("cancelled"), "{err}");
    }
}
