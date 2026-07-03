//! `todos` domain — `bennu_todos`, powering the TODO tool window.
//!
//! A line-oriented scan of the project's `.java` / `.xml` / `.jsp` / `.properties`
//! files under `root`. For each line it matches a case-sensitive marker word
//! (`TODO` | `FIXME` | `XXX` | `HACK`) with word boundaries (so `TODOLIST` doesn't
//! trip `TODO`), like IntelliJ's default TODO patterns — no comment-context parsing.
//! Emits one [`TodoItem`] per matched line (marker + trimmed remainder).
//!
//! The walk skips `target/`, `.git/`, `node_modules/`, `.idea/` and caps at
//! [`MAX_TODOS`] results (logging to stderr when capped, never erroring). No `regex`
//! crate — a plain word-boundary substring scan keeps the module dependency-free.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::TodoItem;
use serde::Deserialize;

/// The marker words recognised (case-sensitive), in the FE's display order.
const MARKERS: [&str; 4] = ["TODO", "FIXME", "XXX", "HACK"];

/// File extensions scanned for TODO markers.
const SCAN_EXTS: [&str; 4] = ["java", "xml", "jsp", "properties"];

/// Directory names never descended into during the scan.
const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", ".idea"];

/// Upper bound on returned items (a huge legacy project can hold many); capping keeps
/// the payload bounded. Logged (not errored) when hit.
const MAX_TODOS: usize = 2000;

/// Max length of the captured trailing `text` per item.
const MAX_TEXT_LEN: usize = 200;

/// Args for [`bennu_todos`].
#[derive(Deserialize)]
pub struct TodoScanArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
}

/// Scan `root` for `TODO`/`FIXME`/`XXX`/`HACK` markers and return the hits.
#[arbor_rpc::handler]
fn bennu_todos(_ctx: &BennuState, args: TodoScanArgs) -> Result<Vec<TodoItem>, String> {
    let mut out = Vec::new();
    let mut capped = false;
    scan_dir(Path::new(&args.root), &mut out, &mut capped);
    if capped {
        eprintln!("bennu-be: bennu_todos capped at {MAX_TODOS} results for {}", args.root);
    }
    Ok(out)
}

/// Recursively walk `dir`, scanning eligible files. Stops adding once `MAX_TODOS` is
/// reached (setting `capped`).
fn scan_dir(dir: &Path, out: &mut Vec<TodoItem>, capped: &mut bool) {
    if out.len() >= MAX_TODOS {
        *capped = true;
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        if out.len() >= MAX_TODOS {
            *capped = true;
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            scan_dir(&path, out, capped);
        } else if is_scannable(&path) {
            scan_file(&path, out, capped);
        }
    }
}

/// Whether `path`'s extension is one of [`SCAN_EXTS`].
fn is_scannable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SCAN_EXTS.contains(&e))
        .unwrap_or(false)
}

/// Scan one file line-by-line for markers.
fn scan_file(path: &Path, out: &mut Vec<TodoItem>, capped: &mut bool) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return; // unreadable / non-UTF-8 — skip
    };
    let file = path.to_string_lossy().replace('\\', "/");
    for (idx, line) in source.lines().enumerate() {
        if out.len() >= MAX_TODOS {
            *capped = true;
            return;
        }
        if let Some((kind, text)) = match_marker(line) {
            out.push(TodoItem { file: file.clone(), line: idx + 1, kind, text });
        }
    }
}

/// Match the first marker word in `line` (word-bounded, case-sensitive) and return
/// its `(kind, text)` where `text` is the trimmed remainder after the marker (and any
/// single `:` / whitespace separator), capped at [`MAX_TEXT_LEN`] chars.
fn match_marker(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    let mut best: Option<(usize, &str)> = None;
    for marker in MARKERS {
        if let Some(pos) = find_word(bytes, marker) {
            if best.map(|(p, _)| pos < p).unwrap_or(true) {
                best = Some((pos, marker));
            }
        }
    }
    let (pos, marker) = best?;
    let after = &line[pos + marker.len()..];
    // Drop a single leading `:` and surrounding whitespace, IntelliJ-style.
    let text = after.trim_start().strip_prefix(':').unwrap_or(after).trim();
    let text: String = text.chars().take(MAX_TEXT_LEN).collect();
    Some((marker.to_string(), text))
}

/// Find the byte offset of the first word-bounded, case-sensitive occurrence of
/// `word` in `haystack` (boundaries: not preceded/followed by `[A-Za-z0-9_]`).
fn find_word(haystack: &[u8], word: &str) -> Option<usize> {
    let w = word.as_bytes();
    if w.is_empty() || haystack.len() < w.len() {
        return None;
    }
    let mut i = 0;
    while i + w.len() <= haystack.len() {
        if &haystack[i..i + w.len()] == w {
            let before_ok = i == 0 || !is_word_byte(haystack[i - 1]);
            let after_idx = i + w.len();
            let after_ok = after_idx >= haystack.len() || !is_word_byte(haystack[after_idx]);
            if before_ok && after_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Whether `b` is part of an ASCII identifier (`[A-Za-z0-9_]`).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_todo_comment_and_extracts_text() {
        let (kind, text) = match_marker("    // TODO: refactor this").unwrap();
        assert_eq!(kind, "TODO");
        assert_eq!(text, "refactor this");
    }

    #[test]
    fn matches_fixme_without_colon() {
        let (kind, text) = match_marker("<!-- FIXME broken link -->").unwrap();
        assert_eq!(kind, "FIXME");
        assert_eq!(text, "broken link -->");
    }

    #[test]
    fn word_bounded_and_case_sensitive() {
        assert!(match_marker("todo lowercase").is_none());
        assert!(match_marker("TODOLIST is not a marker").is_none());
        assert!(match_marker("// XXX").is_some());
    }
}
