//! The per-file revision log, and the path index that makes the logs findable.
//!
//! One append-only `.jsonl` per file, named by the hash of its project-relative path.
//! Append-only because the common operation — "record what this file just became" — is
//! then a single write that cannot corrupt what came before it, whatever happens
//! halfway through. Rewriting only happens when something is deliberately removed
//! (a purge, a label), and then it goes through a temp file and a rename.
//!
//! A log is named by a hash, so the store cannot enumerate its own contents by looking
//! at filenames. [`Index`] is the answer: an append-only `hash → path` map, read once
//! per store and consulted for every question that starts with "which files…".

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::blobs;
use crate::error::HistoryResult;
use crate::model::Revision;

/// Where a file's log lives under the store root.
pub fn log_path(root: &Path, path_key: &str) -> PathBuf {
    root.join("log").join(format!("{path_key}.jsonl"))
}

/// Read a file's revisions, oldest first. A missing log is an empty history, not an
/// error: every file starts without one.
pub fn read(root: &Path, path_key: &str) -> Vec<Revision> {
    let Ok(text) = std::fs::read_to_string(log_path(root, path_key)) else { return Vec::new() };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        // A line this build cannot parse is skipped rather than fatal. A log is a record
        // of the past: refusing to read all of it because one line is from the future
        // would throw away the part that is perfectly readable.
        .filter_map(|l| serde_json::from_str::<Revision>(l).ok())
        .collect()
}

/// Append one revision.
pub fn append(root: &Path, path_key: &str, rev: &Revision) -> HistoryResult<()> {
    let path = log_path(root, path_key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(rev)?)?;
    Ok(())
}

/// Replace a file's revisions wholesale — the only non-append operation, used by a
/// purge and by labelling. Writing an empty list removes the log.
pub fn rewrite(root: &Path, path_key: &str, revs: &[Revision]) -> HistoryResult<()> {
    let path = log_path(root, path_key);
    if revs.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut body = String::new();
    for r in revs {
        body.push_str(&serde_json::to_string(r)?);
        body.push('\n');
    }
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct IndexLine {
    h: String,
    p: String,
}

/// The `hash → project-relative path` map.
///
/// Append-only on disk and deduplicated on read, so registering a path that is already
/// known is a no-op the caller does not have to check for.
#[derive(Debug, Default)]
pub struct Index {
    map: BTreeMap<String, String>,
}

impl Index {
    pub fn load(root: &Path) -> Self {
        let mut map = BTreeMap::new();
        if let Ok(text) = std::fs::read_to_string(root.join("paths.jsonl")) {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(e) = serde_json::from_str::<IndexLine>(line) {
                    map.insert(e.h, e.p);
                }
            }
        }
        Self { map }
    }

    /// Register `rel` if it is new. Returns its key either way.
    pub fn register(&mut self, root: &Path, rel: &str) -> HistoryResult<String> {
        let key = blobs::key(rel);
        if self.map.contains_key(&key) {
            return Ok(key);
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("paths.jsonl"))?;
        writeln!(f, "{}", serde_json::to_string(&IndexLine { h: key.clone(), p: rel.to_string() })?)?;
        self.map.insert(key.clone(), rel.to_string());
        Ok(key)
    }

    /// Every tracked path, with its key.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
