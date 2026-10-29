//! What the user's own Run console printed, kept in the backend so the assistant can read it.
//!
//! The console is drawn by the frontend from `run-output` events and the backend kept nothing: a run
//! that finished, or a trace that scrolled away, existed only in the webview. This is the smallest
//! sound mechanism — the same lines the events carry (escapes already removed), appended to a bounded
//! ring per run, a few runs per project — fed by the pumps in `build.rs` and read, never written, by
//! the assistant's `console_logs` / `log_facets`.
//!
//! Bounded three ways: lines and bytes per run (the `LogBuffer` caps), runs per project
//! ([`RUNS_PER_PROJECT`]), and everything of a project goes when the project is released.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use bennu_live::prelude::{now_ms, LogBuffer, LogLine, LogQuery, Stream};

/// Runs remembered per project; the oldest finished one goes first.
const RUNS_PER_PROJECT: usize = 6;
const MAX_LINES: usize = 5_000;
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// One run's output, shared by its two pumps and the readers.
pub(crate) struct ConsoleRun {
    id: String,
    root: String,
    label: String,
    command: String,
    started_ms: u64,
    state: Mutex<State>,
    log: Mutex<LogBuffer>,
}

#[derive(Default)]
struct State {
    ended_ms: Option<u64>,
    exit_code: Option<i32>,
}

/// What a reader is told about a run.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunInfo {
    pub id: String,
    /// `r3`: the short name a person (or the model) can type.
    pub short: String,
    pub label: String,
    pub command: String,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub lines: usize,
    pub dropped: u64,
}

impl RunInfo {
    pub(crate) fn status(&self) -> String {
        match (self.ended_ms, self.exit_code) {
            (None, _) => "running".into(),
            (Some(_), Some(code)) => format!("exited with code {code}"),
            (Some(_), None) => "ended (killed or stopped)".into(),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn registry() -> &'static Mutex<Vec<Arc<ConsoleRun>>> {
    static REG: OnceLock<Mutex<Vec<Arc<ConsoleRun>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Vec::new()))
}

/// `run-3-1785933181000000000` -> `r3`.
fn short_of(id: &str) -> String {
    id.strip_prefix("run-").and_then(|r| r.split('-').next()).map(|n| format!("r{n}")).unwrap_or_else(|| id.to_string())
}

/// Start remembering a run. Returns the handle the pumps write to.
pub(crate) fn begin(root: &str, run_id: &str, label: &str, command: &str) -> Arc<ConsoleRun> {
    let run = Arc::new(ConsoleRun {
        id: run_id.to_string(),
        root: key(root),
        label: label.to_string(),
        command: command.to_string(),
        started_ms: now_ms(),
        state: Mutex::new(State::default()),
        log: Mutex::new(LogBuffer::new(MAX_LINES, MAX_BYTES)),
    });
    let mut all = lock(registry());
    all.push(Arc::clone(&run));
    // Keep the newest RUNS_PER_PROJECT of this project: a running run is never the one dropped.
    let mut mine: Vec<usize> = all.iter().enumerate().filter(|(_, r)| r.root == run.root).map(|(i, _)| i).collect();
    while mine.len() > RUNS_PER_PROJECT {
        match mine.iter().position(|&i| lock(&all[i].state).ended_ms.is_some()) {
            Some(p) => {
                all.remove(mine[p]);
                mine = all.iter().enumerate().filter(|(_, r)| r.root == run.root).map(|(i, _)| i).collect();
            }
            None => break,
        }
    }
    run
}

/// Normalised so `C:\p` and `C:/p/` are one project.
fn key(root: &str) -> String {
    root.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

impl ConsoleRun {
    pub(crate) fn push(&self, stream: Stream, text: &str) {
        lock(&self.log).push(now_ms(), stream, text);
    }

    pub(crate) fn finish(&self, code: Option<i32>) {
        let mut s = lock(&self.state);
        s.ended_ms = Some(now_ms());
        s.exit_code = code;
    }

    fn info(&self) -> RunInfo {
        let s = lock(&self.state);
        let log = lock(&self.log);
        RunInfo {
            id: self.id.clone(),
            short: short_of(&self.id),
            label: self.label.clone(),
            command: self.command.clone(),
            started_ms: self.started_ms,
            ended_ms: s.ended_ms,
            exit_code: s.exit_code,
            lines: log.len(),
            dropped: log.dropped(),
        }
    }
}

/// The runs remembered for a project, newest first.
pub(crate) fn list(root: &str) -> Vec<RunInfo> {
    let k = key(root);
    let mut runs: Vec<RunInfo> = lock(registry()).iter().filter(|r| r.root == k).map(|r| r.info()).collect();
    // Insertion order is start order, and unlike the clock it never ties.
    runs.reverse();
    runs
}

/// A run's lines. `selector`: `None` is the newest run; otherwise `r3`, `3`, the full id or the run's label.
pub(crate) fn read(root: &str, selector: Option<&str>, query: &LogQuery) -> Result<(RunInfo, Vec<LogLine>), String> {
    let k = key(root);
    let all = lock(registry());
    let mut mine: Vec<&Arc<ConsoleRun>> = all.iter().filter(|r| r.root == k).collect();
    mine.reverse();
    let chosen = match selector.map(str::trim).filter(|s| !s.is_empty()) {
        None => mine.first().copied(),
        Some(sel) => {
            let short = if sel.bytes().all(|b| b.is_ascii_digit()) { format!("r{sel}") } else { sel.to_string() };
            mine.iter().copied().find(|r| r.id == sel || short_of(&r.id).eq_ignore_ascii_case(&short) || r.label.eq_ignore_ascii_case(sel))
        }
    };
    let run = chosen.ok_or_else(|| match selector {
        None => "the Run console has no run in this project yet: the user starts one from the Run button".to_string(),
        Some(s) => format!("no run `{s}` (known: {})", mine.iter().map(|r| format!("{} [{}]", short_of(&r.id), r.label)).collect::<Vec<_>>().join(", ")),
    })?;
    let lines = lock(&run.log).query(query);
    Ok((run.info(), lines))
}

/// Forget a project's runs (the project was released).
pub(crate) fn forget(root: &str) {
    let k = key(root);
    lock(registry()).retain(|r| r.root != k);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_listed_with_its_status_and_read_back() {
        let run = begin("C:\\proj-a", "run-1-100", "com.acme.Main", "java -cp x com.acme.Main");
        run.push(Stream::Stdout, "hello");
        run.push(Stream::Stderr, "boom");
        assert_eq!(list("c:/proj-a/")[0].status(), "running");
        run.finish(Some(1));
        let (info, lines) = read("C:/proj-a", None, &LogQuery::default()).unwrap();
        assert_eq!((info.short.as_str(), info.status().as_str(), lines.len()), ("r1", "exited with code 1", 2));
        forget("C:/proj-a");
        assert!(list("C:/proj-a").is_empty());
    }

    #[test]
    fn only_the_newest_runs_of_a_project_are_kept_and_others_are_untouched() {
        let other = begin("/proj-b-other", "run-900-1", "Other", "");
        other.finish(Some(0));
        for n in 0..(RUNS_PER_PROJECT + 3) {
            let r = begin("/proj-b", &format!("run-{}-1", 100 + n), "Main", "");
            r.finish(Some(0));
        }
        let runs = list("/proj-b");
        assert_eq!(runs.len(), RUNS_PER_PROJECT);
        assert!(runs.iter().all(|r| r.short != "r100"));
        assert_eq!(list("/proj-b-other").len(), 1);
        forget("/proj-b");
        forget("/proj-b-other");
    }

    #[test]
    fn a_run_is_selected_by_short_name_number_or_label() {
        let a = begin("/proj-c", "run-501-1", "First", "");
        a.push(Stream::Stdout, "one");
        let b = begin("/proj-c", "run-502-1", "Second", "");
        b.push(Stream::Stdout, "two");
        let q = LogQuery::default();
        assert_eq!(read("/proj-c", Some("r501"), &q).unwrap().1[0].text, "one");
        assert_eq!(read("/proj-c", Some("502"), &q).unwrap().1[0].text, "two");
        assert_eq!(read("/proj-c", Some("first"), &q).unwrap().1[0].text, "one");
        assert!(read("/proj-c", Some("nope"), &q).unwrap_err().contains("known: r502"));
        forget("/proj-c");
        assert!(read("/proj-c", None, &q).unwrap_err().contains("no run"));
    }
}
