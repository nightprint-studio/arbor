//! `Task` — a piece of background work that appears in the Jobs panel, and cannot be left there running.
//!
//! Every long thing the backend does for the person — downloading, resolving, indexing — wants the same
//! four things from the Jobs panel: a row while it runs, its output, a way to stop it where stopping is
//! possible, and an honest ending. They used to be written by hand at each site (register, remember to
//! finish, finish on every path, `Option<JobHandle>` threaded through), and the hand-written part is where
//! it went wrong: a path that returned early left a row *running* for as long as the process lived.
//!
//! ## What a `Task` guarantees
//!
//! - **It always ends.** Dropping a task that was not finished closes its row — as failed, saying it ended
//!   without a result, or saying a panic stopped it. No early `return`, `?` or panic leaves a zombie.
//! - **Silence without a reverse channel.** Without one (a backend run on its own) every method is a no-op,
//!   so the work does not need to know whether anyone is watching.
//! - **Short work leaves no trace.** [`TaskSpec::deferred`] registers the row only if the work is still
//!   running after a delay. Registering is a synchronous call over the reverse channel, and a hundred
//!   operations of a few milliseconds each would fill the panel with rows that were over before anyone could
//!   read them. What the work said before the row existed is kept and shown when it appears, and a deferred
//!   task that *fails* registers at once, so a failure is never invisible because it was fast.
//! - **Cancelling means something.** [`Task::on_cancel`] says what stopping this work does; without it the
//!   row has no Cancel button, which is the honest answer for work that cannot be stopped.
//!
//! The registry itself is the shell's — see [`crate::jobs`]. This is the part that sits in front of it.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::json;

use crate::index_service::{finish_bennu_job, register_job};
use crate::jobs::JobHandle;

/// Most lines kept for a row that does not exist yet.
const BACKLOG_LINES: usize = 500;

/// What a task is, as the Jobs panel shows it.
#[derive(Clone)]
pub(crate) struct TaskSpec {
    name: String,
    /// What it is doing, in a line — a command, a path, a URL.
    command: String,
    category: String,
    /// An ephemeral pass: its row goes away shortly after it succeeds. A failure stays.
    system: bool,
    /// Registered with a Cancel button. Set by [`Task::on_cancel`]'s caller, never on its own.
    cancellable: bool,
    defer: Option<Duration>,
}

impl TaskSpec {
    pub(crate) fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        TaskSpec {
            name: name.into(),
            command: command.into(),
            category: "Task".to_string(),
            system: false,
            cancellable: false,
            defer: None,
        }
    }

    /// The group the row is listed under (`Download`, `Dependencies`, …).
    pub(crate) fn category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// A background pass the person did not ask for: it clears itself when it succeeds.
    pub(crate) fn system(mut self) -> Self {
        self.system = true;
        self
    }

    /// The row gets a Cancel button. The task must then say what it does with [`Task::on_cancel`].
    pub(crate) fn cancellable(mut self) -> Self {
        self.cancellable = true;
        self
    }

    /// Register the row only if the work outlasts `after`.
    pub(crate) fn deferred(mut self, after: Duration) -> Self {
        self.defer = Some(after);
        self
    }
}

struct State {
    job: Option<JobHandle>,
    finished: bool,
    /// What the work said before there was a row to put it in.
    backlog: Vec<String>,
    /// Set before the row existed; applied to it when it does.
    on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// A unit of background work in the Jobs panel. See the module docs.
pub(crate) struct Task {
    host: Option<Arc<dyn HostCaller>>,
    sink: Arc<dyn EventSink>,
    spec: Arc<TaskSpec>,
    state: Arc<Mutex<State>>,
    /// Set once the task has been closed, so the `Drop` that follows does not close it a second time.
    closed: AtomicBool,
    last_percent: AtomicUsize,
}

impl Task {
    /// Start a task. Without `host` (no reverse channel) it is a task that nobody can see, and every method
    /// does nothing.
    pub(crate) fn start(host: Option<&Arc<dyn HostCaller>>, sink: &Arc<dyn EventSink>, spec: TaskSpec) -> Task {
        let task = Task {
            host: host.cloned(),
            sink: Arc::clone(sink),
            spec: Arc::new(spec),
            state: Arc::new(Mutex::new(State { job: None, finished: false, backlog: Vec::new(), on_cancel: None })),
            closed: AtomicBool::new(false),
            // Not 0: a first report of 0 % should still be said.
            last_percent: AtomicUsize::new(usize::MAX),
        };
        match task.spec.defer {
            None => task.register(),
            Some(after) => {
                let (host, sink, spec, state) =
                    (task.host.clone(), Arc::clone(&task.sink), Arc::clone(&task.spec), Arc::clone(&task.state));
                std::thread::spawn(move || {
                    std::thread::sleep(after);
                    register(host.as_ref(), &sink, &spec, &state);
                });
            }
        }
        task
    }

    fn register(&self) {
        register(self.host.as_ref(), &self.sink, &self.spec, &self.state);
    }

    /// Say a line in the task's output.
    pub(crate) fn line(&self, text: &str) {
        self.log().line(text);
    }

    /// A handle that can say lines after the task itself has moved on — into a callback that outlives the borrow, say.
    pub(crate) fn log(&self) -> TaskLog {
        TaskLog { sink: Arc::clone(&self.sink), state: Arc::clone(&self.state) }
    }

    /// Report `done` of `total` as one line per whole percentage point — at most a hundred for the whole run,
    /// which is what a person watching a progress readout reads, and not a line per file: callers report from
    /// worker threads thousands of times, and the reverse channel is not for that.
    pub(crate) fn progress(&self, label: &str, done: usize, total: usize) {
        if total == 0 {
            return;
        }
        let percent = done * 100 / total;
        // `swap`, not load-then-store: two workers reporting together must not both say the same percentage.
        if self.last_percent.swap(percent, Ordering::Relaxed) == percent {
            return;
        }
        self.line(&format!("{label}… {percent}% ({done}/{total})"));
    }

    /// What cancelling this task does. Gives the row its Cancel button — the spec must be
    /// [`cancellable`](TaskSpec::cancellable), or the panel does not offer it.
    pub(crate) fn on_cancel(&self, action: impl Fn() + Send + Sync + 'static) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match &state.job {
            Some(job) => job.on_cancel(action),
            None => state.on_cancel = Some(Arc::new(action)),
        }
    }

    /// Whether the person cancelled it.
    pub(crate) fn is_cancelled(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.job.as_ref().is_some_and(JobHandle::is_cancelled)
    }

    /// The work is over, and went well.
    pub(crate) fn ok(self) {
        self.close(true, None);
    }

    /// The work is over, and `reason` is why it did not.
    pub(crate) fn fail(self, reason: impl Into<String>) {
        self.close(false, Some(reason.into()));
    }

    /// The work is over: well on `Ok`, with the error's text on `Err`.
    pub(crate) fn finish(self, result: Result<(), String>) {
        match result {
            Ok(()) => self.ok(),
            Err(reason) => self.fail(reason),
        }
    }

    fn close(&self, success: bool, error: Option<String>) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // A fast failure still gets its row: it is the one thing that must never be invisible.
        if !success {
            self.register();
        }
        let job = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.finished = true;
            state.job.take()
        };
        // Outside the lock: finishing talks to the shell.
        finish_bennu_job(&self.sink, job, success, error);
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            let why = if std::thread::panicking() { "Stopped by a panic." } else { "Ended without a result." };
            self.close(false, Some(why.to_string()));
        }
    }
}

/// Says lines in a task's output. Cheap to clone; stays valid, and does nothing, once the task is over.
#[derive(Clone)]
pub(crate) struct TaskLog {
    sink: Arc<dyn EventSink>,
    state: Arc<Mutex<State>>,
}

impl TaskLog {
    pub(crate) fn line(&self, text: &str) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match &state.job {
            Some(job) => say(&self.sink, job, text),
            None if !state.finished && state.backlog.len() < BACKLOG_LINES => state.backlog.push(text.to_string()),
            None => {}
        }
    }
}

/// Register the row, once, and hand it what the task said and asked for before it existed.
fn register(host: Option<&Arc<dyn HostCaller>>, sink: &Arc<dyn EventSink>, spec: &TaskSpec, state: &Mutex<State>) {
    let Some(host) = host else { return };
    let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
    if state.job.is_some() || state.finished {
        return;
    }
    let Some(job) = register_job(host, sink, &spec.name, &spec.command, &spec.category, spec.system, !spec.cancellable) else {
        return;
    };
    if let Some(action) = state.on_cancel.take() {
        job.on_cancel(move || action());
    }
    for line in state.backlog.drain(..) {
        say(sink, &job, &line);
    }
    state.job = Some(job);
}

/// One line into the row's buffer and onto the live stream the open panel renders.
fn say(sink: &Arc<dyn EventSink>, job: &JobHandle, text: &str) {
    job.append(text);
    sink.emit("arbor://job-output-batch", json!({ "job_id": &job.id, "lines": [text] }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hears nothing, says nothing.
    struct Quiet;

    impl EventSink for Quiet {
        fn emit(&self, _topic: &str, _payload: serde_json::Value) {}
    }

    /// Without a reverse channel there is nothing to register with, and nothing may go wrong.
    #[test]
    fn a_task_nobody_can_see_does_nothing_and_does_not_panic() {
        let sink: Arc<dyn EventSink> = Arc::new(Quiet);
        let task = Task::start(None, &sink, TaskSpec::new("x", "y").deferred(Duration::from_millis(5)));
        task.line("said before anything exists");
        task.progress("Working", 1, 2);
        task.on_cancel(|| {});
        assert!(!task.is_cancelled());
        task.ok();

        // Dropped without a verdict: closed all the same.
        let dropped = Task::start(None, &sink, TaskSpec::new("x", "y"));
        drop(dropped);
    }

    #[test]
    fn progress_says_each_percentage_once() {
        let sink: Arc<dyn EventSink> = Arc::new(Quiet);
        let task = Task::start(None, &sink, TaskSpec::new("x", "y"));
        // The same percentage twice, and a total of nothing, are not lines.
        task.progress("Analyzing", 1, 1000);
        task.progress("Analyzing", 2, 1000);
        task.progress("Analyzing", 5, 0);
        assert_eq!(task.last_percent.load(Ordering::Relaxed), 0);
        task.progress("Analyzing", 500, 1000);
        assert_eq!(task.last_percent.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn the_spec_reads_as_it_was_built() {
        let spec = TaskSpec::new("Download", "mvn").category("Download").system().cancellable().deferred(Duration::from_secs(1));
        assert_eq!((spec.category.as_str(), spec.system, spec.cancellable), ("Download", true, true));
        assert_eq!(spec.defer, Some(Duration::from_secs(1)));
    }
}
