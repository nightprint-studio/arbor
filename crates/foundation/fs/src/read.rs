//! Reading: directory listing, recursive name search, and text-file read.
//!
//! All functions are synchronous and blocking — the shell wraps them in
//! `spawn_blocking` so a slow network drive can't stall the IPC runtime.

use std::path::{Path, PathBuf};

use crate::entry::{to_unix_ms, FsEntry};
use crate::error::{FsError, Result};

/// List the contents of a directory with full metadata. Hidden entries
/// (starting with `.`) are excluded unless `show_hidden` is set.
pub fn read_dir(path: &str, show_hidden: bool) -> Result<Vec<FsEntry>> {
    let base = Path::new(path);

    let read = std::fs::read_dir(base)
        .map_err(|e| FsError::io("Cannot read directory", e))?;

    let mut entries: Vec<FsEntry> = Vec::new();

    for item in read {
        let Ok(item) = item else { continue };

        let name = item.file_name().to_string_lossy().to_string();

        // Skip hidden entries (dot-prefixed) when not explicitly requested.
        if !show_hidden && name.starts_with('.') { continue; }

        let full_path = item.path();
        let path_str  = full_path.to_string_lossy().to_string();

        let (is_dir, size, modified, created) = match item.metadata() {
            Ok(meta) => {
                let is_dir = meta.is_dir();
                let size   = if is_dir { None } else { Some(meta.len()) };
                (is_dir, size, to_unix_ms(meta.modified()), to_unix_ms(meta.created()))
            }
            Err(_) => (full_path.is_dir(), None, None, None),
        };

        entries.push(FsEntry { name, path: path_str, is_dir, size, modified, created });
    }

    Ok(entries)
}

/// Build a file-name matcher from `query`. A query containing `*` / `?` is
/// treated as a (case-insensitive, anchored) glob; otherwise it's a
/// case-insensitive substring match. An empty query matches everything.
fn build_name_matcher(query: &str) -> Box<dyn Fn(&str) -> bool + Send> {
    let t = query.trim();
    if t.is_empty() {
        return Box::new(|_| true);
    }
    if t.contains('*') || t.contains('?') {
        let mut re = String::from("(?i)^");
        for ch in t.chars() {
            match ch {
                '*' => re.push_str(".*"),
                '?' => re.push('.'),
                '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                    re.push('\\');
                    re.push(ch);
                }
                _ => re.push(ch),
            }
        }
        re.push('$');
        if let Ok(r) = regex::Regex::new(&re) {
            return Box::new(move |name: &str| r.is_match(name));
        }
    }
    let lc = t.to_lowercase();
    Box::new(move |name: &str| name.to_lowercase().contains(&lc))
}

/// Recursively search `root` for entries whose **file name** matches `query`
/// (glob when it contains `*`/`?`, else case-insensitive substring). Returns up
/// to `limit` matches so a huge tree can't run away. Hidden entries
/// (dot-prefixed) are skipped unless `show_hidden`.
pub fn search(root: &str, query: &str, show_hidden: bool, limit: usize) -> Result<Vec<FsEntry>> {
    let matches = build_name_matcher(query);
    let mut out: Vec<FsEntry> = Vec::new();
    // Depth-first walk with an explicit stack (no recursion → no stack
    // blow-up on deep trees, and a cheap early-out at the cap).
    let mut stack: Vec<PathBuf> = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for item in rd {
            let Ok(item) = item else { continue };
            let name = item.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let full = item.path();
            let meta = item.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            if is_dir {
                stack.push(full.clone());
            }
            if matches(&name) {
                let size = meta.as_ref().and_then(|m| if m.is_dir() { None } else { Some(m.len()) });
                let modified = meta.as_ref().and_then(|m| to_unix_ms(m.modified()));
                let created  = meta.as_ref().and_then(|m| to_unix_ms(m.created()));
                out.push(FsEntry {
                    name,
                    path: full.to_string_lossy().to_string(),
                    is_dir,
                    size,
                    modified,
                    created,
                });
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// Read a text file from disk and return its contents as a UTF-8 string.
/// Errors out for non-UTF-8 files; suitable for JSON / TOML / config files.
pub fn read_text(path: &str) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| FsError::io("Cannot read file", e))
}
