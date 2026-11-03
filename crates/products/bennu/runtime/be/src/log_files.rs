//! `log_files` domain — the log files a project has on disk, read the way the Run console reads output.
//!
//! A project's logs do not all go to the console: Tomcat writes `catalina`, `localhost` and access
//! logs to its own folder, and an application's file appender writes wherever its configuration
//! says. This finds them ([`bennu_log_files`]) and **follows** one ([`bennu_log_tail_start`]),
//! sending every line through the same [`LogAnnotator`] the console uses — so a line from a file has
//! the same levels, links to the source, stack-trace folding and SQL as a line from a run.
//!
//! ## Following a file
//!
//! A thread per followed file: it reads the last [`INITIAL_BYTES`] first (the tail is what anyone
//! opens a log for), then polls for growth. A file that shrinks was truncated or rotated, and is read
//! again from its start. Polling, not a file-system watcher: a watcher is a dependency, and half a
//! second of latency is invisible on a log.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use serde::Serialize;
use serde_json::json;

use crate::log::LogAnnotator;

const EVT_LOG_TAIL: &str = "arbor://bennu/log-tail";
/// How much of the end of a file is shown when it is opened.
const INITIAL_BYTES: u64 = 256 * 1024;
/// The most read in one go, so a file that grew by gigabytes between two polls is taken in pieces.
const MAX_READ: u64 = 4 * 1024 * 1024;
/// Lines per event: a large file arrives as a few events, not one a line.
const BATCH: usize = 500;
const POLL: Duration = Duration::from_millis(100);
const POLLS_PER_CHECK: u32 = 4;

const MAX_DEPTH: usize = 4;
const MAX_FILES: usize = 300;

/// A log file of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LogFile {
    /// Absolute.
    pub path: String,
    pub name: String,
    /// Relative to the project root, with `/`.
    pub rel: String,
    /// `tomcat` | `project`.
    pub group: &'static str,
    pub size: u64,
    /// Milliseconds since the epoch.
    pub modified: u64,
}

/// The log files of the project at `root`, newest first within a group, Tomcat's before the rest.
#[arbor_rpc::handler]
fn bennu_log_files(_state: &BennuState, root: String) -> Result<Vec<LogFile>, String> {
    Ok(discover(Path::new(&root)))
}

pub(crate) fn discover(root: &Path) -> Vec<LogFile> {
    let mut found: Vec<PathBuf> = Vec::new();

    // Tomcat's own generated bases keep their logs in `.arbor/bennu/tomcat/<configuration>/logs`.
    let bases = root.join(".arbor/bennu/tomcat");
    if let Ok(entries) = std::fs::read_dir(&bases) {
        for base in entries.filter_map(Result::ok).map(|e| e.path()) {
            collect_files(&base.join("logs"), &mut found);
        }
    }
    walk(root, 0, false, &mut found);
    for declared in crate::log_pattern::declared_files(root) {
        if !found.contains(&declared) {
            found.push(declared);
        }
    }

    let mut files: Vec<LogFile> = found.iter().filter_map(|p| describe(root, &bases, p)).collect();
    files.sort_by(|a, b| a.group.cmp(b.group).reverse().then(b.modified.cmp(&a.modified)).then_with(|| a.name.cmp(&b.name)));
    files.truncate(MAX_FILES);
    files
}

fn describe(root: &Path, tomcat: &Path, path: &Path) -> Option<LogFile> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
    Some(LogFile {
        path: path.to_string_lossy().into_owned(),
        name: path.file_name()?.to_string_lossy().into_owned(),
        rel: path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/"),
        group: if path.starts_with(tomcat) { "tomcat" } else { "project" },
        size: meta.len(),
        modified,
    })
}

/// Walk the project for log files: any `*.log` (or a rotated `*.log.1`), and every file of a folder
/// named `logs` / `log`.
fn walk(dir: &Path, depth: usize, in_logs: bool, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if skipped_dir(&name) {
                continue;
            }
            let logs = matches!(name.to_ascii_lowercase().as_str(), "logs" | "log");
            walk(&path, depth + 1, in_logs || logs, out);
        } else if kind.is_file() && looks_like_log(&name, in_logs) && !out.contains(&path) {
            out.push(path);
        }
    }
}

/// Every log-looking file directly in `dir`.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && !out.contains(&path) {
            out.push(path);
        }
    }
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "build" | "out" | "dist" | "bin" | "obj")
}

/// A name that is a log: `*.log`, a rotation of one (`app.log.1`, `app.log.2026-11-03`), or — inside a
/// `logs` folder — anything text-like (`catalina.out`, `localhost_access_log.2026-11-03.txt`).
fn looks_like_log(name: &str, in_logs: bool) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".log") || lower.contains(".log.") {
        return !lower.ends_with(".gz") && !lower.ends_with(".zip");
    }
    in_logs && (lower.ends_with(".txt") || lower.ends_with(".out") || !lower.contains('.'))
}

// ── following a file ────────────────────────────────────────────────────────────

fn tails() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static TAILS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    TAILS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start following `path`; its lines arrive as `arbor://bennu/log-tail` events carrying the returned id.
#[arbor_rpc::handler]
fn bennu_log_tail_start(state: &BennuState, root: String, path: String) -> Result<String, String> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Err(format!("Not a file: {path}"));
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = format!("lt-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let stop = Arc::new(AtomicBool::new(false));
    tails().lock().map_err(|e| e.to_string())?.insert(id.clone(), stop.clone());

    let sink = state.event_sink();
    let thread_id = id.clone();
    std::thread::spawn(move || {
        follow(&root, &file, &thread_id, &stop, sink.as_ref());
        if let Ok(mut all) = tails().lock() {
            all.remove(&thread_id);
        }
    });
    Ok(id)
}

#[arbor_rpc::handler]
fn bennu_log_tail_stop(_state: &BennuState, id: String) -> Result<(), String> {
    if let Some(stop) = tails().lock().map_err(|e| e.to_string())?.get(&id) {
        stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn follow(root: &str, file: &Path, id: &str, stop: &AtomicBool, sink: &dyn EventSink) {
    let mut annotator = LogAnnotator::for_root(root);
    let mut pos: u64 = 0;
    let mut first = true;
    let mut carry: Vec<u8> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        if let Ok(len) = std::fs::metadata(file).map(|m| m.len()) {
            if len < pos {
                // Truncated or rotated under the same name: start over, and tell the reader.
                pos = 0;
                carry.clear();
                annotator = LogAnnotator::for_root(root);
                sink.emit(EVT_LOG_TAIL, json!({ "id": id, "reset": true, "lines": [] }));
            }
            let start = if first { len.saturating_sub(INITIAL_BYTES) } else { pos };
            if len > start {
                if let Some(chunk) = read_range(file, start, (len - start).min(MAX_READ)) {
                    pos = start + chunk.len() as u64;
                    // Opening in the middle of a file: the first piece is the end of a line we did not see.
                    let skip_first = first && start > 0;
                    emit_lines(sink, id, &mut annotator, &mut carry, chunk, skip_first);
                }
            }
            first = false;
        }
        for _ in 0..POLLS_PER_CHECK {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(POLL);
        }
    }
}

fn read_range(file: &Path, start: u64, len: u64) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(file).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(len as usize);
    f.take(len).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Complete lines of `carry + chunk` go out; what follows the last newline stays in `carry`.
fn emit_lines(sink: &dyn EventSink, id: &str, annotator: &mut LogAnnotator, carry: &mut Vec<u8>, chunk: Vec<u8>, skip_first: bool) {
    carry.extend_from_slice(&chunk);
    let Some(last_newline) = carry.iter().rposition(|b| *b == b'\n') else { return };
    let complete: Vec<u8> = carry.drain(..=last_newline).collect();
    let text = String::from_utf8_lossy(&complete);
    let mut lines = split_lines(&text);
    if skip_first && !lines.is_empty() {
        lines.remove(0);
    }
    for batch in lines.chunks(BATCH) {
        let annotated: Vec<_> = batch.iter().map(|l| annotator.line(l)).collect();
        sink.emit(EVT_LOG_TAIL, json!({ "id": id, "reset": false, "lines": annotated }));
    }
}

/// The lines of text that ends with a newline, without their `\r\n` / `\n`.
fn split_lines(text: &str) -> Vec<&str> {
    text.strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_is_named_like_one() {
        assert!(looks_like_log("app.log", false));
        assert!(looks_like_log("app.log.1", false));
        assert!(looks_like_log("app.log.2026-11-03", false));
        assert!(!looks_like_log("app.log.gz", false));
        assert!(!looks_like_log("notes.txt", false));
        assert!(!looks_like_log("pom.xml", false));
    }

    #[test]
    fn inside_a_logs_folder_text_files_count() {
        assert!(looks_like_log("catalina.out", true));
        assert!(looks_like_log("localhost_access_log.2026-11-03.txt", true));
        assert!(looks_like_log("stdout", true));
        assert!(!looks_like_log("rotate.sh", true));
    }

    #[test]
    fn lines_lose_their_endings() {
        assert_eq!(split_lines("a\r\nb\n"), ["a", "b"]);
        assert_eq!(split_lines("a\n\nb\n"), ["a", "", "b"]);
    }

    #[test]
    fn discovery_finds_logs_folders_loose_logs_and_tomcat_bases() {
        let dir = std::env::temp_dir().join(format!("bennu-log-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["logs", "src/main/java", "target/logs", ".arbor/bennu/tomcat/cfg1/logs"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        for file in [
            "logs/app.log",
            "logs/app.log.1",
            "logs/archive.gz",
            "service.log",
            "target/logs/ignored.log",
            "src/main/java/Foo.java",
            ".arbor/bennu/tomcat/cfg1/logs/catalina.2026-11-03.log",
        ] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        let found = discover(&dir);
        let rels: Vec<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        assert!(rels.contains(&"logs/app.log") && rels.contains(&"logs/app.log.1") && rels.contains(&"service.log"));
        assert!(rels.contains(&".arbor/bennu/tomcat/cfg1/logs/catalina.2026-11-03.log"));
        assert!(!rels.iter().any(|r| r.contains("ignored") || r.ends_with(".gz") || r.ends_with(".java")));
        assert_eq!(found[0].group, "tomcat", "Tomcat's logs come first");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
