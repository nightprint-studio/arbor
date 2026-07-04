//! Incremental, persistable **include-graph cache** — so the form analysis doesn't re-parse
//! every JSP in the project on each editor tab switch.
//!
//! The [`crate::include_graph`] builder is O(all JSPs): it parses the include references of
//! every file to assemble the forward/reverse edge maps. Rebuilding that on every
//! form-analysis request (once per JSP you open) is the hot cost. This cache keeps the
//! per-file edge lists keyed by path, each stamped with the file's `(mtime, size)`, and
//! rebuilds the adjacency maps from them **without touching the filesystem**. Re-parsing is
//! then incremental — only a file whose stamp changed is re-read:
//!
//!   - [`IncludeGraphCache::refresh_file`] re-parses ONE file's edges if its stamp moved — the
//!     cheap per-tab path (the file you just opened / edited-and-saved);
//!   - [`IncludeGraphCache::sync`] does a full freshness pass over the current JSP set
//!     (add/update changed, drop deleted) — the first build + the manual Refresh path.
//!
//! The `files` map is `serde`-serializable so the be-layer can persist it to disk and warm-start
//! across app restarts (only files whose stamp changed since the saved cache are re-parsed). The
//! assembled `graph` is derived (not persisted) — [`IncludeGraphCache::rebuild_after_load`]
//! reconstructs it from the loaded edge lists.
//!
//! Pure over the filesystem + [`crate::jsp_includes`] — unit-tested off temp fixtures.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::include_graph::{key_of, IncludeGraph};
use crate::jsp_includes::{parse_jsp_includes_file, resolve_include_target};

/// One cached file's include edges + the stamp that tells us when to re-parse it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CachedFile {
    /// Last-modified time in milliseconds since the Unix epoch (0 if unavailable).
    mtime: u64,
    /// File size in bytes — paired with `mtime` so a same-second overwrite of a different
    /// length is still caught (mtime resolution can be coarse).
    size: u64,
    /// The resolved forward include targets (forward-slashed keys) of this file.
    targets: Vec<String>,
}

/// The include-graph cache: per-file edge lists (persisted) + the assembled graph (derived).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncludeGraphCache {
    /// `file key → its cached edges`. The persisted source of truth.
    files: HashMap<String, CachedFile>,
    /// The assembled forward/reverse graph — rebuilt from `files`, never persisted.
    #[serde(skip)]
    graph: IncludeGraph,
    /// Set when `files` changed since the last [`Self::commit`]; a commit rebuilds `graph`.
    #[serde(skip)]
    dirty: bool,
}

/// The current `(mtime_ms, size)` stamp of `file`, or `(0, 0)` when unavailable.
pub fn file_stamp(file: &Path) -> (u64, u64) {
    match std::fs::metadata(file) {
        Ok(meta) => {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            (mtime, meta.len())
        }
        Err(_) => (0, 0),
    }
}

impl IncludeGraphCache {
    /// The assembled graph (valid after a [`Self::commit`] / [`Self::sync`] / load+rebuild).
    pub fn graph(&self) -> &IncludeGraph {
        &self.graph
    }

    /// True before anything has been cached (a cold cache → the caller does a full [`Self::sync`]).
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Ensure `file`'s cached edges reflect the stamp `(mtime, size)`, re-parsing its includes
    /// ONLY if the file is new or its stamp moved. Returns true when the edges changed (the
    /// caller should [`Self::commit`] + persist). Cheap per-tab path.
    pub fn refresh_file(&mut self, file: &Path, mtime: u64, size: u64) -> bool {
        let key = key_of(file);
        if let Some(existing) = self.files.get(&key) {
            if existing.mtime == mtime && existing.size == size {
                return false; // unchanged — no re-parse
            }
        }
        let targets = resolve_targets(file);
        self.files.insert(key, CachedFile { mtime, size, targets });
        self.dirty = true;
        true
    }

    /// Full freshness pass over the current JSP set (each `(path, mtime, size)`): re-parse any
    /// new/changed file, drop files no longer present, and [`Self::commit`] if anything moved.
    /// Returns true when the cache changed (the caller should persist).
    pub fn sync(&mut self, current: &[(PathBuf, u64, u64)]) -> bool {
        let mut present: HashSet<String> = HashSet::with_capacity(current.len());
        let mut changed = false;

        for (path, mtime, size) in current {
            present.insert(key_of(path));
            if self.refresh_file(path, *mtime, *size) {
                changed = true;
            }
        }
        // Drop entries for files that vanished (deleted / renamed).
        let before = self.files.len();
        self.files.retain(|k, _| present.contains(k));
        if self.files.len() != before {
            self.dirty = true;
            changed = true;
        }

        self.commit();
        changed
    }

    /// Rebuild the assembled graph from `files` if it's dirty (no filesystem access).
    pub fn commit(&mut self) {
        if self.dirty {
            self.rebuild_graph();
            self.dirty = false;
        }
    }

    /// Reconstruct the assembled graph after deserializing (the graph isn't persisted).
    pub fn rebuild_after_load(&mut self) {
        self.rebuild_graph();
        self.dirty = false;
    }

    /// Assemble the forward/reverse graph from the per-file edge lists.
    fn rebuild_graph(&mut self) {
        let mut graph = IncludeGraph::default();
        for (from, cached) in &self.files {
            for to in &cached.targets {
                graph.add_edge(from, to);
            }
        }
        self.graph = graph;
    }
}

/// Parse `file`'s static include references and resolve each to its on-disk target key. Computed
/// (`${…}`) / external / unresolved references are dropped — only real, navigable edges cached.
fn resolve_targets(file: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for inc in parse_jsp_includes_file(file) {
        if inc.computed {
            continue;
        }
        if let Some(target) = resolve_include_target(file, &inc.raw) {
            let key = key_of(&target);
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tmp_dir;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    /// Build a `(path, mtime, size)` triple set from real files (their live stamps).
    fn current(files: &[PathBuf]) -> Vec<(PathBuf, u64, u64)> {
        files.iter().map(|p| { let (m, s) = file_stamp(p); (p.clone(), m, s) }).collect()
    }

    #[test]
    fn sync_builds_the_same_edges_as_a_fresh_graph() {
        let dir = tmp_dir("cache");
        let a = write(&dir, "a.jspf", "<div/>");
        let page = write(&dir, "page.jsp", r#"<jsp:include page="a.jspf"/>"#);

        let mut cache = IncludeGraphCache::default();
        assert!(cache.is_empty());
        assert!(cache.sync(&current(&[page.clone(), a.clone()])));

        let fresh = crate::include_graph::build_include_graph(&[page.clone(), a.clone()]);
        assert_eq!(cache.graph(), &fresh, "cache graph must equal a from-scratch build");
    }

    #[test]
    fn unchanged_file_is_not_reparsed() {
        let dir = tmp_dir("cache");
        let a = write(&dir, "a.jspf", "<div/>");
        let page = write(&dir, "page.jsp", r#"<jsp:include page="a.jspf"/>"#);

        let mut cache = IncludeGraphCache::default();
        cache.sync(&current(&[page.clone(), a.clone()]));
        let (m, s) = file_stamp(&page);
        // Same stamp → refresh_file reports "no change" and skips the re-parse.
        assert!(!cache.refresh_file(&page, m, s));
    }

    #[test]
    fn changed_stamp_reparses_and_updates_edges() {
        let dir = tmp_dir("cache");
        let a = write(&dir, "a.jspf", "<div/>");
        let b = write(&dir, "b.jspf", "<div/>");
        let page = write(&dir, "page.jsp", r#"<jsp:include page="a.jspf"/>"#);

        let mut cache = IncludeGraphCache::default();
        cache.sync(&current(&[page.clone(), a.clone(), b.clone()]));
        assert_eq!(cache.graph().forward.get(&key_of(&page)).unwrap(), &[key_of(&a)]);

        // Rewrite page to include b instead; refresh with a bumped stamp.
        std::fs::write(&page, r#"<jsp:include page="b.jspf"/>"#).unwrap();
        assert!(cache.refresh_file(&page, 999_999, 42));
        cache.commit();
        assert_eq!(cache.graph().forward.get(&key_of(&page)).unwrap(), &[key_of(&b)]);
    }

    #[test]
    fn sync_drops_a_removed_file_outgoing_edges() {
        // A file removed from the project set loses its OWN (outgoing) edges. Incoming edges from
        // still-unchanged files persist until those are re-parsed — expected cache behaviour.
        let dir = tmp_dir("cache");
        let leaf = write(&dir, "leaf.jspf", "<div/>");
        let mid = write(&dir, "mid.jspf", r#"<jsp:include page="leaf.jspf"/>"#);
        let page = write(&dir, "page.jsp", r#"<jsp:include page="mid.jspf"/>"#);

        let mut cache = IncludeGraphCache::default();
        cache.sync(&current(&[page.clone(), mid.clone(), leaf.clone()]));
        assert_eq!(cache.graph().forward.get(&key_of(&mid)).unwrap(), &[key_of(&leaf)]);

        // `mid` vanishes from the set → its outgoing edge (mid→leaf) is dropped, and `sync`
        // reports the change.
        assert!(cache.sync(&current(&[page.clone(), leaf.clone()])));
        assert!(!cache.graph().forward.contains_key(&key_of(&mid)), "mid's own edges dropped");
    }

    #[test]
    fn survives_a_serde_round_trip() {
        let dir = tmp_dir("cache");
        let a = write(&dir, "a.jspf", "<div/>");
        let page = write(&dir, "page.jsp", r#"<jsp:include page="a.jspf"/>"#);

        let mut cache = IncludeGraphCache::default();
        cache.sync(&current(&[page.clone(), a.clone()]));

        let json = serde_json::to_string(&cache).unwrap();
        let mut loaded: IncludeGraphCache = serde_json::from_str(&json).unwrap();
        // The graph isn't persisted — rebuild it from the loaded edge lists.
        loaded.rebuild_after_load();
        assert_eq!(loaded.graph(), cache.graph());
    }
}
