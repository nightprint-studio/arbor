//! Every tool call, whether it ran or not.
//!
//! An AI surface without a log is one the user can only reason about by trusting a
//! description of it. This is also the first place to look when a model does something
//! surprising — which is usually not "the tool misbehaved" but "the tool was called with
//! arguments nobody expected".
//!
//! ## Kept across restarts
//!
//! This file used to argue for memory only, on the grounds that persisting it means a file
//! accumulating the paths and arguments of everything an assistant ever looked at. The
//! concern was real and the conclusion was wrong: a record you lose on every restart is one
//! you cannot consult about the thing you noticed yesterday, which is most of what a record
//! is for.
//!
//! What answers the concern is *where* and *what*, not *whether*. It goes in the user's own
//! profile directory, beside the settings and the recents — which already hold the paths of
//! every project they open — never in a repository, where it would be committed. It is
//! capped at [`CAPACITY`] entries, so it is a window and not an archive. And **Clear**
//! deletes the file, so "forget what I have been doing" remains one click.
//!
//! Entries carry the run that produced them, so "this session" stays distinguishable from
//! "the ones before" without keeping two logs.
//!
//! ## An entry exists before it finishes
//!
//! A row is opened when the call arrives and closed when it ends, rather than written
//! once at the end. The difference is the whole point of watching: a log that only shows
//! completed calls is silent for exactly as long as something is running, which is the
//! only time anyone is actually looking. A test run that takes two minutes used to be two
//! minutes of an empty screen followed by a row saying it had happened.
//!
//! While a call is open it also collects what the backend says about itself — the same
//! `arbor://progress` lines an AI client is sent — so "what is it doing right now" has an
//! answer here and not only on the wire.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use arbor_core::prelude::arbor_profile_path;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// How many entries are kept. Enough to cover a working session; small enough that the
/// launcher can render the whole thing without paging.
const CAPACITY: usize = 500;

/// How many progress lines one call keeps. A build says thousands; what a reader wants is
/// the last few and the fact that it is still moving.
const PROGRESS_LINES: usize = 40;

/// One recorded call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Per-run id, so a row opened now can be closed later.
    ///
    /// **Only unique within a run**: the counter restarts with the process, so a row read
    /// off disk can carry the same id as one opened a second ago. The identity of a row is
    /// `(run, id)` — everything that looks one up matches on both, and so must every
    /// consumer. Matching on `id` alone silently addresses a previous session's row.
    pub id: u64,
    /// Which run produced this. The process's start time, so it orders naturally and two
    /// runs cannot collide.
    #[serde(default)]
    pub run: u64,
    /// Unix milliseconds.
    pub at: u64,
    pub tool: String,
    /// The backend that served it.
    pub program: String,
    /// `read` / `write` / `destructive`.
    pub safety: String,
    /// Where the call is: `waiting` (arrived, not yet decided), `asking` (in front of
    /// the user's consent prompt), `running`, or how it ended — `allowed`,
    /// `asked_allowed`, `asked_denied`, `denied`, `timed_out`, `failed`, `interrupted`.
    pub outcome: String,
    /// Compact JSON of the arguments, truncated. Kept because the arguments are the
    /// interesting part of a surprising call.
    pub arguments: String,
    /// Milliseconds spent in the backend, when it ran.
    pub duration_ms: Option<u64>,
    /// The error, when the call failed or was refused.
    pub detail: Option<String>,
    /// What the backend has said about itself while running, oldest first, capped.
    pub progress: Vec<String>,
}

impl AuditEntry {
    /// A row for a call that has just arrived and has not been decided yet.
    pub fn opening(tool: String, program: String, safety: String, arguments: String) -> Self {
        Self {
            id: next_id(),
            run: current_run(),
            at: now_ms(),
            tool,
            program,
            safety,
            outcome: "waiting".into(),
            arguments,
            duration_ms: None,
            detail: None,
            progress: Vec::new(),
        }
    }
}

static LOG: Mutex<Vec<AuditEntry>> = Mutex::new(Vec::new());

/// This process's run id — its start time, which orders naturally and cannot collide with
/// another run's.
pub fn current_run() -> u64 {
    static RUN: OnceLock<u64> = OnceLock::new();
    *RUN.get_or_init(now_ms)
}

/// Where the log lives: the active profile, beside `profile.toml`.
fn log_path() -> PathBuf {
    arbor_profile_path("mcp-activity.json")
}

/// Read the previous runs' entries in, once, before the first read or write.
///
/// A failure here is silence on purpose: a log that would not load is a reason to have no
/// history, never a reason for the endpoint not to work.
fn ensure_loaded() {
    static LOADED: OnceLock<()> = OnceLock::new();
    LOADED.get_or_init(|| {
        let Ok(text) = std::fs::read_to_string(log_path()) else { return };
        let Ok(mut stored) = serde_json::from_str::<Vec<AuditEntry>>(&text) else { return };
        // A row is written out while it is still open on purpose — a call that was running
        // when Arbor stopped is worth seeing, and is sometimes why it stopped. But the
        // process that would have closed it is gone, so on the way back in it is read for
        // what it is: interrupted, not still going. Left alone it would sit in "in flight"
        // for the rest of time, and the one state the panel exists to show would be a lie.
        for entry in &mut stored {
            if LIVE.contains(&entry.outcome.as_str()) {
                entry.outcome = "interrupted".to_string();
            }
        }
        if let Ok(mut log) = LOG.lock() {
            // Oldest first, matching the in-memory order, and this run's entries land
            // after them because this happens before any of them exist.
            stored.truncate(CAPACITY);
            stored.extend(log.drain(..));
            dedupe(&mut stored);
            *log = stored;
        }
    });
}

/// Write the log out. Called when a row reaches a final state, not on every update: a
/// running call produces a line of progress a second, and a file rewritten that often
/// would be a disk write per line for a row that is not the record yet.
fn persist() {
    let snapshot = { LOG.lock().map(|l| l.clone()).unwrap_or_default() };
    let Ok(text) = serde_json::to_string(&snapshot) else { return };
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// Outcomes a row can still move on from. Anything else is where it stopped.
const LIVE: &[&str] = &["waiting", "asking", "running"];

fn next_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Put a row on the log now, before the call has been decided or run.
pub fn open(app: &AppHandle, entry: &AuditEntry) {
    ensure_loaded();
    if let Ok(mut log) = LOG.lock() {
        if log.len() >= CAPACITY {
            log.remove(0);
        }
        log.push(entry.clone());
    }
    push(app, entry);
}

/// Replace an open row with its final state. Falls back to appending, so a row that was
/// evicted by the cap while its call ran still ends up recorded rather than lost.
pub fn record(app: &AppHandle, entry: AuditEntry) {
    ensure_loaded();
    if let Ok(mut log) = LOG.lock() {
        match log.iter_mut().find(|e| e.id == entry.id && e.run == entry.run) {
            // The progress the row collected belongs to the call, not to whoever is
            // closing it — the caller never sees those lines and would blank them.
            Some(open) => {
                let progress = std::mem::take(&mut open.progress);
                *open = entry.clone();
                open.progress = progress;
            }
            None => {
                if log.len() >= CAPACITY {
                    log.remove(0);
                }
                log.push(entry.clone());
            }
        }
    }
    push(app, &entry);
    // The row has reached a final state, so now it is the record.
    persist();
}

/// Note that an open call has said something about itself.
pub fn progress(app: &AppHandle, id: u64, line: &str) {
    let updated = {
        let Ok(mut log) = LOG.lock() else { return };
        let run = current_run();
        let Some(entry) = log.iter_mut().find(|e| e.id == id && e.run == run) else { return };
        // The first line of progress is also the proof it got past the gates.
        if entry.outcome == "waiting" {
            entry.outcome = "running".into();
        }
        if entry.progress.len() >= PROGRESS_LINES {
            entry.progress.remove(0);
        }
        entry.progress.push(line.to_string());
        entry.clone()
    };
    push(app, &updated);
}

/// Mark an open call as past the gates and into the backend.
pub fn running(app: &AppHandle, id: u64) {
    mark(app, id, "running");
}

/// Mark an open call as parked in front of the user's consent prompt.
///
/// Its own state rather than a shade of "waiting": this is the one a reader can act on,
/// and it can last as long as the prompt's timeout.
pub fn asking(app: &AppHandle, id: u64) {
    mark(app, id, "asking");
}

fn mark(app: &AppHandle, id: u64, outcome: &str) {
    let updated = {
        let Ok(mut log) = LOG.lock() else { return };
        let run = current_run();
        let Some(entry) = log.iter_mut().find(|e| e.id == id && e.run == run) else { return };
        entry.outcome = outcome.to_string();
        entry.clone()
    };
    push(app, &updated);
}

/// Best-effort: the launcher window may not be open, and a call must never fail because
/// nobody was watching it.
fn push(app: &AppHandle, entry: &AuditEntry) {
    let _ = app.emit("arbor://mcp-call", entry.clone());
}

/// Collapse rows that share `(run, id)`, keeping the one that still knows something.
///
/// The identity of a row is `(run, id)` and the file is not owned by one process: two
/// Arbor instances on the same profile both hydrate it and both rewrite it whole, so it
/// can come back holding one call twice — once finished, once as it looked while it was
/// still open and later read back as `interrupted`. Reading is where that gets absorbed,
/// because it is the only place that sees the whole list at once.
///
/// A finished row beats a live one; between two of the same kind the one that collected
/// more progress wins. Order is preserved: the survivor keeps the earliest position, so
/// the log still reads chronologically.
fn dedupe(rows: &mut Vec<AuditEntry>) {
    let mut seen: std::collections::HashMap<(u64, u64), usize> = std::collections::HashMap::new();
    let mut out: Vec<AuditEntry> = Vec::with_capacity(rows.len());
    for row in rows.drain(..) {
        match seen.get(&(row.run, row.id)) {
            None => {
                seen.insert((row.run, row.id), out.len());
                out.push(row);
            }
            Some(&at) => {
                let kept = &out[at];
                let kept_live = LIVE.contains(&kept.outcome.as_str());
                let row_live = LIVE.contains(&row.outcome.as_str());
                let replace = match (kept_live, row_live) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => row.progress.len() > kept.progress.len(),
                };
                if replace {
                    out[at] = row;
                }
            }
        }
    }
    *rows = out;
}

/// The log, newest first, with the run that is reading it.
pub fn entries() -> ActivityLog {
    ensure_loaded();
    let mut out = LOG.lock().map(|l| l.clone()).unwrap_or_default();
    dedupe(&mut out);
    out.reverse();
    ActivityLog { run: current_run(), entries: out }
}

/// The log plus the reader's own run, so "this session" is answerable without the caller
/// guessing which run id is the live one.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityLog {
    pub run: u64,
    pub entries: Vec<AuditEntry>,
}

/// Forget everything. Offered because the log holds file paths, and a user handing over
/// a screen should be able to clear it.
pub fn clear() {
    if let Ok(mut log) = LOG.lock() {
        log.clear();
    }
    // Removed, not emptied: "forget what I have been doing" should not leave a file behind
    // that says how much there was to forget.
    let _ = std::fs::remove_file(log_path());
}

/// Unix milliseconds, or 0 if the clock is before the epoch (it is not).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Compact JSON, truncated to something a log row can hold.
pub fn preview(arguments: &serde_json::Value) -> String {
    let text = arguments.to_string();
    if text.len() <= 400 {
        return text;
    }
    // Cut on a char boundary — arguments carry paths, and paths carry accents.
    let mut end = 400;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &text[..end], text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_that_outlived_its_process_is_not_still_running() {
        // Nothing will ever close it: the process that would have is gone. Reading it back
        // as "running" leaves the panel claiming work is in flight for the rest of time.
        let live = ["waiting", "asking", "running"];
        for outcome in live {
            assert!(LIVE.contains(&outcome), "{outcome} must be recognised as open");
        }
        for outcome in ["allowed", "denied", "failed", "interrupted"] {
            assert!(!LIVE.contains(&outcome), "{outcome} must be recognised as finished");
        }
    }

    #[test]
    fn a_row_is_identified_by_its_run_as_well_as_its_id() {
        // The id counter restarts with the process, so a row read off disk and one opened
        // now collide on `id` alone. Everything that looks a row up must match both, or it
        // closes a previous session's call instead of this one's — and a UI keyed on `id`
        // alone crashes on the duplicate.
        let mine = AuditEntry::opening("t".into(), "bennu".into(), "read".into(), "{}".into());
        let stored = AuditEntry { run: mine.run - 1, ..mine.clone() };
        assert_eq!(mine.id, stored.id, "ids do collide across runs");
        assert_ne!((mine.run, mine.id), (stored.run, stored.id));
    }

    #[test]
    fn a_run_id_is_stable_within_a_process() {
        // It is what separates this session's rows from the ones read off disk, so a
        // second call returning a second id would make every row look like its own run.
        assert_eq!(current_run(), current_run());
        assert!(current_run() > 0);
    }

    #[test]
    fn an_opening_row_carries_the_run_and_is_not_finished() {
        let entry = AuditEntry::opening("t".into(), "bennu".into(), "read".into(), "{}".into());
        assert_eq!(entry.run, current_run());
        assert_eq!(entry.outcome, "waiting");
        assert!(entry.progress.is_empty());
        assert!(entry.duration_ms.is_none());
    }

    #[test]
    fn a_stored_row_round_trips() {
        // The file is the record across restarts, so a shape that does not survive serde
        // is a history that silently empties on upgrade.
        let entry = AuditEntry::opening("t".into(), "bennu".into(), "read".into(), "{}".into());
        let text = serde_json::to_string(&vec![entry.clone()]).unwrap();
        let back: Vec<AuditEntry> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].id, entry.id);
        assert_eq!(back[0].run, entry.run);
        assert_eq!(back[0].tool, "t");
    }

    #[test]
    fn preview_truncates_on_a_char_boundary() {
        let args = serde_json::json!({ "root": "é".repeat(500) });
        let p = preview(&args);
        assert!(p.ends_with(')'));
        // The truncation must not have split a multi-byte character.
        assert!(p.is_char_boundary(p.len()));
    }

    #[test]
    fn short_arguments_are_kept_whole() {
        let args = serde_json::json!({ "root": "/p" });
        assert_eq!(preview(&args), r#"{"root":"/p"}"#);
    }

    /// A row with just enough shape for the identity + survivor rules.
    fn row(run: u64, id: u64, outcome: &str, progress: usize) -> AuditEntry {
        AuditEntry {
            id,
            run,
            at: 0,
            tool: "t".to_string(),
            program: "p".to_string(),
            safety: "read".to_string(),
            outcome: outcome.to_string(),
            arguments: String::new(),
            duration_ms: None,
            detail: None,
            progress: (0..progress).map(|n| n.to_string()).collect(),
        }
    }

    #[test]
    fn dedupe_keeps_the_finished_copy_over_the_interrupted_one() {
        // Exactly what a log written by two instances looked like on disk: the same
        // call, once completed with its progress and once as it was left open.
        let mut rows = vec![
            row(1787129283433, 1, "asked_allowed", 3),
            row(1787129283433, 1, "interrupted", 0),
            row(1787129903858, 1, "allowed", 0),
        ];
        dedupe(&mut rows);
        assert_eq!(rows.len(), 2, "the duplicate identity collapses");
        assert_eq!(rows[0].outcome, "asked_allowed", "a finished row beats a live one");
        assert_eq!(rows[0].progress.len(), 3, "and keeps what it collected");
        // `(run, id)` is the identity: the same id under a different run is a different call.
        assert_eq!(rows[1].run, 1787129903858);
    }

    #[test]
    fn dedupe_prefers_the_copy_that_saw_more() {
        let mut rows = vec![row(9, 1, "allowed", 0), row(9, 1, "allowed", 4)];
        dedupe(&mut rows);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].progress.len(), 4, "between two finished rows, the richer one");
    }

    #[test]
    fn dedupe_preserves_order_and_leaves_a_clean_log_alone() {
        let mut rows = vec![row(1, 1, "allowed", 0), row(1, 2, "denied", 0), row(2, 1, "allowed", 0)];
        let before: Vec<(u64, u64)> = rows.iter().map(|r| (r.run, r.id)).collect();
        dedupe(&mut rows);
        let after: Vec<(u64, u64)> = rows.iter().map(|r| (r.run, r.id)).collect();
        assert_eq!(before, after, "nothing to collapse, nothing reordered");
    }
}
