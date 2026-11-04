//! `todos` domain — `bennu_todos`, powering the TODO tool window.
//!
//! A line-oriented scan of the project's `.java` / `.xml` / `.jsp` / `.properties`
//! files under `root`. For each line it matches a case-sensitive marker word
//! (`TODO` | `FIXME` | `XXX` | `HACK`) with word boundaries (so `TODOLIST` doesn't
//! trip `TODO`), like IntelliJ's default TODO patterns — no comment-context parsing.
//! Emits one [`TodoItem`] per matched line (marker + trimmed remainder).
//!
//! The walk skips the same noise directories as the project tree (`target/`, `.git/`,
//! `node_modules/`, build output, …), visits entries in name order so a capped result is the same
//! every time, and caps at [`MAX_TODOS`] results — reported through [`TodoScan::truncated`].
//! No `regex` crate — a plain word-boundary substring scan keeps the module dependency-free.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_project::prelude::is_tree_noise_dir;
use bennu_proto::prelude::TodoItem;
use serde::{Deserialize, Serialize};

/// The marker words recognised (case-sensitive), in the FE's display order.
const MARKERS: [&str; 4] = ["TODO", "FIXME", "XXX", "HACK"];

/// File extensions scanned for TODO markers. `rs` / `toml` / `dig` are here so the TODO
/// tool isn't empty on a Cargo project — a panel that shows nothing reads as "no TODOs",
/// not as "this file type isn't scanned".
const SCAN_EXTS: [&str; 7] = ["java", "xml", "jsp", "properties", "rs", "toml", "dig"];

/// Upper bound on returned items (a huge legacy project can hold many); capping keeps
/// the payload bounded. Reported through [`TodoScan::truncated`] when hit.
const MAX_TODOS: usize = 2000;

/// Max length of the captured trailing `text` per item.
const MAX_TEXT_LEN: usize = 200;

/// Args for [`bennu_todos`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct TodoScanArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
    /// The marker words to look for (letters, digits, `_`). Absent or empty: `TODO`, `FIXME`, `XXX`, `HACK`.
    /// The editor passes the words it colours, so the window lists exactly what the editor marks.
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    /// Whether a marker must be written exactly as listed. Absent: it must.
    #[serde(default)]
    pub case_sensitive: Option<bool>,
}

/// The words being looked for and how strictly.
struct Markers {
    words: Vec<String>,
    case_sensitive: bool,
}

impl Markers {
    fn from_args(args: &TodoScanArgs) -> Self {
        let words: Vec<String> = args
            .keywords
            .iter()
            .flatten()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty() && w.bytes().all(is_word_byte))
            .collect();
        let words = if words.is_empty() { MARKERS.iter().map(|m| m.to_string()).collect() } else { words };
        Markers { words, case_sensitive: args.case_sensitive.unwrap_or(true) }
    }
}

/// What [`bennu_todos`] returns: the markers, and whether there were more than fit.
#[derive(Debug, Serialize)]
pub struct TodoScan {
    /// Markers in path order, then line order.
    pub items: Vec<TodoItem>,
    /// `true` when the scan stopped at the cap: `items` is the first part of the list, not all of it.
    pub truncated: bool,
}

/// Find every TODO / FIXME / XXX / HACK marker left in the project's source, with its
/// file, line and the text of the comment.
///
/// Returns `{ items, truncated }`: at most 2,000 markers, in path order, with `truncated` set when
/// there were more. Build output (`target`, `build`/`dist`/`out` beside a manifest, …) and
/// dependency directories are not scanned. Errors when `root` is not a directory.
#[arbor_rpc::handler(mcp(
    title = "Find TODO and FIXME markers",
    safety = read,
))]
fn bennu_todos(_ctx: &BennuState, args: TodoScanArgs) -> Result<TodoScan, String> {
    let root = Path::new(&args.root);
    if !root.is_dir() {
        return Err(format!("{} is not a directory", args.root));
    }
    let markers = Markers::from_args(&args);
    let mut items = Vec::new();
    let mut truncated = false;
    scan_dir(root, &markers, &mut items, &mut truncated);
    Ok(TodoScan { items, truncated })
}

/// Recursively walk `dir` in name order, scanning eligible files. Stops adding once `MAX_TODOS` is
/// reached (setting `capped`).
///
/// Sorted because `read_dir` order is whatever the filesystem likes: with a cap, an unsorted walk
/// returns a different 2,000 markers from one call to the next. Typed by `file_type()`, which does not
/// follow links, so a symlink loop cannot recurse.
fn scan_dir(dir: &Path, markers: &Markers, out: &mut Vec<TodoItem>, capped: &mut bool) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MAX_TODOS {
            *capped = true;
            return;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            if !is_tree_noise_dir(dir, &entry.file_name().to_string_lossy()) {
                scan_dir(&path, markers, out, capped);
            }
        } else if kind.is_file() && is_scannable(&path) {
            scan_file(&path, markers, out, capped);
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
fn scan_file(path: &Path, markers: &Markers, out: &mut Vec<TodoItem>, capped: &mut bool) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return; // unreadable / non-UTF-8 — skip
    };
    let file = path.to_string_lossy().replace('\\', "/");
    for (idx, line) in source.lines().enumerate() {
        if let Some((kind, text)) = match_marker(line, markers) {
            // Checked on a hit, not per line: a full list only means "truncated" if there is one more.
            if out.len() >= MAX_TODOS {
                *capped = true;
                return;
            }
            out.push(TodoItem { file: file.clone(), line: idx + 1, kind, text });
        }
    }
}

/// Match the first marker word in `line` (word-bounded, case-sensitive) and return
/// its `(kind, text)` where `text` is the trimmed remainder after the marker (and any
/// single `:` / whitespace separator), capped at [`MAX_TEXT_LEN`] chars.
fn match_marker(line: &str, markers: &Markers) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    let mut best: Option<(usize, &str)> = None;
    for marker in &markers.words {
        if let Some(pos) = find_word(bytes, marker, markers.case_sensitive) {
            if best.map(|(p, _)| pos < p).unwrap_or(true) {
                best = Some((pos, marker));
            }
        }
    }
    let (pos, marker) = best?;
    let after = &line[pos + marker.len()..];
    // `TODO(alice): …` — the author in brackets belongs to the marker, not to the text.
    let after = after.strip_prefix('(').and_then(|r| r.split_once(')')).map_or(after, |(_, rest)| rest);
    // Drop a single leading `:` and surrounding whitespace, IntelliJ-style.
    let text = after.trim_start().strip_prefix(':').unwrap_or(after).trim();
    let text: String = text.chars().take(MAX_TEXT_LEN).collect();
    // One kind per word whatever its spelling, so the window's filter chips do not split `todo` from `TODO`.
    let kind = if markers.case_sensitive { marker.to_string() } else { marker.to_ascii_uppercase() };
    Some((kind, text))
}

/// Find the byte offset of the first word-bounded occurrence of `word` in `haystack` (boundaries:
/// not preceded/followed by `[A-Za-z0-9_]`), matching case only when `case_sensitive`.
fn find_word(haystack: &[u8], word: &str, case_sensitive: bool) -> Option<usize> {
    let w = word.as_bytes();
    if w.is_empty() || haystack.len() < w.len() {
        return None;
    }
    let mut i = 0;
    while i + w.len() <= haystack.len() {
        let same = if case_sensitive { &haystack[i..i + w.len()] == w } else { haystack[i..i + w.len()].eq_ignore_ascii_case(w) };
        if same {
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

    fn default_markers() -> Markers {
        Markers { words: MARKERS.iter().map(|m| m.to_string()).collect(), case_sensitive: true }
    }

    fn markers(words: &[&str], case_sensitive: bool) -> Markers {
        Markers { words: words.iter().map(|w| w.to_string()).collect(), case_sensitive }
    }

    fn match_marker(line: &str) -> Option<(String, String)> {
        super::match_marker(line, &default_markers())
    }

    #[test]
    fn configured_words_replace_the_defaults_and_can_ignore_case() {
        let m = markers(&["NOTE", "BUG"], false);
        assert_eq!(super::match_marker("// note: mind the order", &m), Some(("NOTE".into(), "mind the order".into())));
        assert!(super::match_marker("// TODO: not listed", &m).is_none());
        assert!(super::match_marker("// note", &markers(&["NOTE"], true)).is_none());
    }

    #[test]
    fn an_author_in_brackets_is_not_part_of_the_text() {
        let (kind, text) = match_marker("// TODO(alice): split this").unwrap();
        assert_eq!((kind.as_str(), text.as_str()), ("TODO", "split this"));
    }

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
    fn a_missing_root_is_an_error_not_an_empty_list() {
        let missing = std::env::temp_dir().join("bennu-todos-no-such-dir");
        assert!(!missing.is_dir());
        let mut items = Vec::new();
        let mut capped = false;
        scan_dir(&missing, &default_markers(), &mut items, &mut capped);
        assert!(items.is_empty() && !capped);
    }

    #[test]
    fn word_bounded_and_case_sensitive() {
        assert!(match_marker("todo lowercase").is_none());
        assert!(match_marker("TODOLIST is not a marker").is_none());
        assert!(match_marker("// XXX").is_some());
    }
}
