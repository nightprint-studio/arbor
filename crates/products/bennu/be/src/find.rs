//! `find` domain — `bennu_find_in_files`, powering the project-wide text search.
//!
//! A **fresh**, line-oriented scan of the project's text files under `root` (no
//! persisted index needed), mirroring [`crate::todos`] / [`crate::class_index`]'s walk:
//! recurse the tree, skip `target/`, `.git/`, `node_modules/`, `.idea/`, skip anything
//! that isn't valid UTF-8, and match a caller-supplied `query` per line.
//!
//! Matching modes ([`FindInFilesArgs`]):
//!   * `regex` — **fallback** to a case-insensitive substring match. The `regex` crate is
//!     NOT a dependency of `bennu-be` (adding it needs approval), so `regex == true` is
//!     honoured as "loose, case-insensitive substring" rather than a true engine. The FE
//!     is free to surface this as a degraded mode.
//!   * plain substring — respects `case_sensitive`.
//!   * `whole_word` — bounds the match on `[A-Za-z0-9_]` word boundaries (so `Foo` does
//!     not match inside `FooBar`).
//!
//! Emits one [`FindHit`] per matched line (first match on the line drives `col`). The walk
//! caps at [`MAX_HITS`] results (logging to stderr when capped, never erroring).

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::FindHit;
use serde::Deserialize;

/// File extensions scanned for text matches. Files with no extension are scanned too when
/// their name starts with `.` (dotfiles like `.gitignore` / `.editorconfig`), handled in
/// [`is_scannable`].
const SCAN_EXTS: [&str; 15] = [
    "java", "xml", "jsp", "jspf", "tag", "properties", "js", "css", "html", "sql", "yml",
    "yaml", "md", "txt", "jspx",
];

/// Directory names never descended into during the scan (mirrors [`crate::todos`]).
const SKIP_DIRS: [&str; 4] = ["target", ".git", "node_modules", ".idea"];

/// Upper bound on returned hits — a project-wide search on a huge legacy tree can match a
/// lot; capping keeps the payload bounded. Logged (not errored) when hit.
const MAX_HITS: usize = 5000;

/// Max length of the captured `preview` per hit (chars, not bytes).
const MAX_PREVIEW_LEN: usize = 300;

/// Args for [`bennu_find_in_files`].
#[derive(Deserialize)]
pub struct FindInFilesArgs {
    /// Absolute path to the project root to scan.
    pub root: String,
    /// The text (or, in `regex` fallback mode, the substring) to find.
    pub query: String,
    /// Regex mode. NOTE: the `regex` crate isn't a dependency, so this falls back to a
    /// case-insensitive substring match (see the module doc).
    #[serde(default)]
    pub regex: bool,
    /// Case-sensitive matching (ignored in `regex` fallback mode, which is always
    /// case-insensitive).
    #[serde(default)]
    pub case_sensitive: bool,
    /// Bound the match on `[A-Za-z0-9_]` word boundaries.
    #[serde(default)]
    pub whole_word: bool,
}

/// The compiled matching policy for one search (derived once from the args, applied per
/// line — avoids re-lowercasing the needle for every line).
struct Matcher {
    /// The needle as searched: lowered when the match is case-insensitive.
    needle: String,
    /// Whether the haystack line must be lowered before searching.
    ci: bool,
    /// Whether to bound the match on word boundaries.
    whole_word: bool,
}

impl Matcher {
    fn new(args: &FindInFilesArgs) -> Self {
        // regex fallback is always case-insensitive; otherwise honour `case_sensitive`.
        let ci = args.regex || !args.case_sensitive;
        let needle = if ci { args.query.to_lowercase() } else { args.query.clone() };
        Self { needle, ci, whole_word: args.whole_word }
    }

    /// The byte offset of the first match of `needle` in `line`, or `None`. When
    /// case-insensitive, the search runs over a lowered copy of the line, but the returned
    /// offset is valid on the ORIGINAL line only when the lowering is length-preserving —
    /// which it is for the ASCII identifiers/keywords this search targets. For a
    /// non-ASCII line whose lowering changes length we fall back to reporting offset 0
    /// (the hit is still surfaced; only the column is approximate).
    fn find(&self, line: &str) -> Option<usize> {
        if self.needle.is_empty() {
            return None;
        }
        if self.ci {
            let lowered = line.to_lowercase();
            let pos = self.find_in(&lowered)?;
            // Column is byte-accurate only when lowering didn't shift byte lengths.
            if lowered.len() == line.len() { Some(pos) } else { Some(0) }
        } else {
            self.find_in(line)
        }
    }

    /// First match offset within an already case-normalised `hay`, honouring `whole_word`.
    fn find_in(&self, hay: &str) -> Option<usize> {
        let hb = hay.as_bytes();
        let nb = self.needle.as_bytes();
        if nb.is_empty() || hb.len() < nb.len() {
            return None;
        }
        let mut i = 0;
        while i + nb.len() <= hb.len() {
            if &hb[i..i + nb.len()] == nb {
                if !self.whole_word || word_bounded(hb, i, nb.len()) {
                    return Some(i);
                }
            }
            i += 1;
        }
        None
    }
}

/// Whether the match at `[start, start+len)` in `hay` is bounded by non-word chars on both
/// sides (word chars: `[A-Za-z0-9_]`).
fn word_bounded(hay: &[u8], start: usize, len: usize) -> bool {
    let before_ok = start == 0 || !is_word_byte(hay[start - 1]);
    let end = start + len;
    let after_ok = end >= hay.len() || !is_word_byte(hay[end]);
    before_ok && after_ok
}

/// Whether `b` is part of an ASCII identifier (`[A-Za-z0-9_]`).
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Project-wide text search: scan `root`'s text files for `query` and return the hits.
#[arbor_rpc::handler]
fn bennu_find_in_files(_ctx: &BennuState, args: FindInFilesArgs) -> Result<Vec<FindHit>, String> {
    let matcher = Matcher::new(&args);
    let mut out = Vec::new();
    let mut capped = false;
    if !args.query.is_empty() {
        scan_dir(Path::new(&args.root), &matcher, &mut out, &mut capped);
    }
    if capped {
        eprintln!(
            "bennu-be: bennu_find_in_files capped at {MAX_HITS} hits for {} (query {:?})",
            args.root, args.query
        );
    }
    Ok(out)
}

/// Recursively walk `dir`, scanning eligible files. Stops once `MAX_HITS` is reached
/// (setting `capped`). Mirrors [`crate::todos::scan_dir`].
fn scan_dir(dir: &Path, matcher: &Matcher, out: &mut Vec<FindHit>, capped: &mut bool) {
    if out.len() >= MAX_HITS {
        *capped = true;
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        if out.len() >= MAX_HITS {
            *capped = true;
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            scan_dir(&path, matcher, out, capped);
        } else if is_scannable(&path) {
            scan_file(&path, matcher, out, capped);
        }
    }
}

/// Whether `path` is a text file we scan: a known extension, or an extension-less dotfile
/// (`.gitignore`, `.editorconfig`, …).
fn is_scannable(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return SCAN_EXTS.contains(&ext);
    }
    // No extension: scan only if it's a dotfile (a plain `Makefile`/binary is skipped).
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') && n.len() > 1)
        .unwrap_or(false)
}

/// Scan one file line-by-line for the matcher's needle.
fn scan_file(path: &Path, matcher: &Matcher, out: &mut Vec<FindHit>, capped: &mut bool) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return; // unreadable / non-UTF-8 — skip
    };
    let file = path.to_string_lossy().replace('\\', "/");
    for (idx, line) in source.lines().enumerate() {
        if out.len() >= MAX_HITS {
            *capped = true;
            return;
        }
        if let Some(byte_col) = matcher.find(line) {
            // Column is 1-based CHAR count up to the byte offset (so a preview containing
            // multi-byte chars reports a caret-friendly column).
            let col = line[..byte_col].chars().count() + 1;
            let preview: String = line.trim().chars().take(MAX_PREVIEW_LEN).collect();
            out.push(FindHit { file: file.clone(), line: idx + 1, col, preview });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(query: &str, regex: bool, case_sensitive: bool, whole_word: bool) -> FindInFilesArgs {
        FindInFilesArgs {
            root: String::new(),
            query: query.to_string(),
            regex,
            case_sensitive,
            whole_word,
        }
    }

    #[test]
    fn substring_case_insensitive_by_default() {
        let m = Matcher::new(&args("todo", false, false, false));
        assert_eq!(m.find("  // TODO: refactor"), Some("  // ".len()));
        assert!(m.find("nothing here").is_none());
    }

    #[test]
    fn substring_case_sensitive_when_asked() {
        let m = Matcher::new(&args("TODO", false, true, false));
        assert!(m.find("todo lowercase").is_none());
        assert_eq!(m.find("a TODO b"), Some(2));
    }

    #[test]
    fn whole_word_bounds_the_match() {
        let m = Matcher::new(&args("foo", false, true, true));
        assert!(m.find("foobar").is_none());
        assert!(m.find("a_foo_b").is_none()); // underscore is a word char
        assert_eq!(m.find("call foo();"), Some("call ".len()));
    }

    #[test]
    fn whole_word_case_insensitive_combo() {
        let m = Matcher::new(&args("Order", false, false, true));
        assert!(m.find("Reorder()").is_none());
        assert_eq!(m.find("new order(x)"), Some("new ".len()));
    }

    #[test]
    fn regex_flag_is_case_insensitive_fallback() {
        // regex==true ignores case_sensitive and matches as a loose substring.
        let m = Matcher::new(&args("HANDLER", true, true, false));
        assert_eq!(m.find("register a handler"), Some("register a ".len()));
    }

    #[test]
    fn empty_query_never_matches() {
        let m = Matcher::new(&args("", false, false, false));
        assert!(m.find("anything").is_none());
    }

    #[test]
    fn dotfiles_and_known_exts_are_scannable() {
        assert!(is_scannable(Path::new("/p/Foo.java")));
        assert!(is_scannable(Path::new("/p/page.jspf")));
        assert!(is_scannable(Path::new("/p/.gitignore")));
        assert!(!is_scannable(Path::new("/p/image.png")));
        assert!(!is_scannable(Path::new("/p/Makefile")));
    }
}
