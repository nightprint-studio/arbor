//! The store: one project's history, and every question you can ask it.
//!
//! ## What it does not know
//!
//! It does not know what a source file is, what git ignores, or which of its callers is
//! an editor. Those are policies, and they differ per product: Bennu skips what git
//! ignores, a note vault would not. So the **caller decides what is worth recording**
//! and this decides only what it can honestly store — content that fits the size ceiling,
//! under a path inside the project it was opened for.
//!
//! ## Bytes, never text
//!
//! A legacy source is Cp1252 and a `.png` is not text at all. Decoding on the way in
//! would mean a restore that gives back something *equivalent* rather than something
//! *identical*, and "equivalent" is not a promise a safety net gets to make.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::blobs;
use crate::error::{HistoryError, HistoryResult};
use crate::log::{self, Index};
use crate::model::{
    ChangeFile, ChangeGroup, DeletedEntry, FileHistory, FolderEntry, HistoryConfig, PurgeReport,
    Revision, RevisionKind, Usage,
};

/// What a recording is part of, beyond the file and its bytes.
///
/// A struct rather than three parameters because two of the three are almost always
/// absent, and `record(p, k, b, None, None, None)` at a call site says nothing about
/// which `None` is which.
#[derive(Debug, Clone, Default)]
pub struct RecordCtx {
    /// The change set id, shared by every file one operation touched.
    pub change: Option<String>,
    /// What the operation was, in words: `"Rename frame_at → frame_at_ms"`.
    pub title: Option<String>,
    /// For a rename, where the file came from (project-relative).
    pub from: Option<String>,
}

impl RecordCtx {
    /// A one-file operation with a description — the common tool-driven case.
    pub fn titled(change: impl Into<String>, title: impl Into<String>) -> Self {
        Self { change: Some(change.into()), title: Some(title.into()), from: None }
    }
}

/// One project's local history.
pub struct HistoryStore {
    project_root: PathBuf,
    dir: PathBuf,
    cfg: HistoryConfig,
}

impl HistoryStore {
    /// Open (creating if needed) the store for `project_root` under `data_root`.
    ///
    /// `data_root` is the product's data directory: the store lives in the user profile,
    /// never inside the project. A history directory inside a repository is a directory
    /// that gets committed, and one inside a working tree is one that a `clean` deletes.
    pub fn open(
        data_root: &Path,
        project_root: &Path,
        cfg: HistoryConfig,
    ) -> HistoryResult<Self> {
        let project_root = project_root.to_path_buf();
        let dir = data_root.join("local-history").join(blobs::key(&norm(&project_root)));
        std::fs::create_dir_all(&dir)?;
        // Written once so the directory is identifiable by a human who finds it: a tree
        // of hashes with no note of what it belongs to is a tree nobody dares delete.
        let meta = dir.join("root.txt");
        if !meta.exists() {
            let _ = std::fs::write(&meta, norm(&project_root));
        }
        Ok(Self { project_root, dir, cfg })
    }

    pub fn config(&self) -> HistoryConfig {
        self.cfg
    }

    /// The project-relative, forward-slash form of `path`.
    pub fn rel(&self, path: &Path) -> HistoryResult<String> {
        let p = norm(path);
        let root = norm(&self.project_root);
        if p == root {
            return Ok(String::new());
        }
        p.strip_prefix(&format!("{root}/"))
            .map(str::to_string)
            .ok_or_else(|| HistoryError::Outside(p))
    }

    /// The absolute path of a project-relative one.
    pub fn abs(&self, rel: &str) -> PathBuf {
        self.project_root.join(rel)
    }

    // ── recording ────────────────────────────────────────────────────────────

    /// Record `bytes` as the state of `path`.
    ///
    /// Returns `None` — not an error — when nothing was recorded, which is the ordinary
    /// outcome in three cases: history is off, the file is over the size ceiling, or the
    /// content is byte-identical to the newest revision. The last one is what keeps a
    /// save-happy editor from filling the timeline with rows that all say the same thing.
    pub fn record(
        &self,
        path: &Path,
        kind: RevisionKind,
        bytes: Option<&[u8]>,
        ctx: &RecordCtx,
    ) -> HistoryResult<Option<Revision>> {
        if !self.cfg.enabled {
            return Ok(None);
        }
        if let Some(b) = bytes {
            if b.len() as u64 > self.cfg.max_file_bytes {
                return Ok(None);
            }
        }
        let rel = self.rel(path)?;
        let mut index = Index::load(&self.dir);
        let key = index.register(&self.dir, &rel)?;
        let existing = log::read(&self.dir, &key);

        let blob = match bytes {
            Some(b) => Some(blobs::put(&self.dir, b)?),
            None => None,
        };
        if let Some(last) = existing.last() {
            // Same content as the newest revision, and neither is a deletion: there is
            // nothing new to say. A deletion followed by the same content IS news — the
            // file came back.
            if kind.has_content() && last.kind.has_content() && last.blob == blob {
                return Ok(None);
            }
            // Two deletions in a row would be the store noticing the same absence twice.
            if !kind.has_content() && !last.kind.has_content() {
                return Ok(None);
            }
        } else if !kind.has_content() {
            // A file we never had content for, reported as deleted. Nothing to restore,
            // so nothing worth a row in a list whose only action is Restore.
            return Ok(None);
        }

        let at = now_ms();
        let rev = Revision {
            id: format!("{at:013}-{:03}", existing.len()),
            at,
            kind,
            size: bytes.map(|b| b.len() as u64).unwrap_or(0),
            blob,
            label: None,
            title: ctx.title.clone(),
            change: ctx.change.clone(),
            from: ctx.from.clone(),
        };
        log::append(&self.dir, &key, &rev)?;
        Ok(Some(rev))
    }

    /// Record what is on disk right now. A path that has vanished records a deletion,
    /// which is what makes an external `rm` show up in the Deleted list.
    pub fn record_from_disk(
        &self,
        path: &Path,
        kind: RevisionKind,
        ctx: &RecordCtx,
    ) -> HistoryResult<Option<Revision>> {
        match std::fs::read(path) {
            Ok(bytes) => self.record(path, kind, Some(&bytes), ctx),
            Err(_) => self.record(path, RevisionKind::Deleted, None, ctx),
        }
    }

    /// Pin a name on a revision. A labelled revision never expires.
    pub fn label(&self, path: &Path, revision: &str, label: &str) -> HistoryResult<()> {
        let rel = self.rel(path)?;
        let key = blobs::key(&rel);
        let mut revs = log::read(&self.dir, &key);
        let found = revs.iter_mut().find(|r| r.id == revision);
        let Some(r) = found else { return Err(HistoryError::NoRevision(revision.to_string())) };
        r.label = Some(label.to_string()).filter(|s| !s.is_empty());
        log::rewrite(&self.dir, &key, &revs)
    }

    // ── reading ──────────────────────────────────────────────────────────────

    /// One file's history, newest first.
    pub fn history(&self, path: &Path) -> HistoryResult<FileHistory> {
        let rel = self.rel(path)?;
        let mut revisions = log::read(&self.dir, &blobs::key(&rel));
        let deleted = revisions.last().map(|r| !r.kind.has_content()).unwrap_or(false);
        revisions.reverse();
        Ok(FileHistory { path: rel, deleted, revisions })
    }

    /// The bytes of one revision.
    pub fn content(&self, path: &Path, revision: &str) -> HistoryResult<Vec<u8>> {
        let rel = self.rel(path)?;
        let revs = log::read(&self.dir, &blobs::key(&rel));
        let rev = revs
            .into_iter()
            .find(|r| r.id == revision)
            .ok_or_else(|| HistoryError::NoRevision(revision.to_string()))?;
        let blob = rev.blob.ok_or_else(|| HistoryError::NoContent(revision.to_string()))?;
        blobs::get(&self.dir, &blob)
    }

    /// The bytes the file had at the newest revision that has any — the content a
    /// deleted file is restored from.
    pub fn last_content(&self, rel: &str) -> HistoryResult<Vec<u8>> {
        let revs = log::read(&self.dir, &blobs::key(rel));
        let blob = revs
            .iter()
            .rev()
            .find_map(|r| r.blob.clone())
            .ok_or_else(|| HistoryError::NoContent(rel.to_string()))?;
        blobs::get(&self.dir, &blob)
    }

    /// Every file the history knows and the project no longer has, newest loss first.
    ///
    /// This is the whole reason the store keeps a path index: a deleted file has no row
    /// in any tree to right-click, so the only way to reach its history is a list that
    /// exists independently of the filesystem.
    pub fn deleted(&self) -> Vec<DeletedEntry> {
        let index = Index::load(&self.dir);
        let mut out: Vec<DeletedEntry> = index
            .entries()
            .filter_map(|(key, rel)| {
                let revs = log::read(&self.dir, key);
                let last = revs.last()?;
                if last.kind.has_content() {
                    return None;
                }
                let previous = revs.iter().rev().find(|r| r.blob.is_some());
                Some(DeletedEntry {
                    name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
                    path: rel.to_string(),
                    at: last.at,
                    kind: last.kind,
                    title: last.title.clone(),
                    blob: previous.and_then(|r| r.blob.clone()),
                    size: previous.map(|r| r.size).unwrap_or(0),
                    revisions: revs.len(),
                })
            })
            .collect();
        out.sort_by(|a, b| b.at.cmp(&a.at));
        out
    }

    /// What the history knows about the direct children of `dir`, as of `at`
    /// (or now). Files and, aggregated, sub-directories.
    pub fn folder(&self, dir: &Path, at: Option<i64>) -> HistoryResult<Vec<FolderEntry>> {
        let base = self.rel(dir)?;
        let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
        let cutoff = at.unwrap_or(i64::MAX);
        let index = Index::load(&self.dir);

        // Sub-directories are folded into one entry each: a folder view that listed one
        // row per file three levels down would be a search result, not a folder.
        let mut dirs: BTreeMap<String, (i64, usize, bool)> = BTreeMap::new();
        let mut files: Vec<FolderEntry> = Vec::new();

        for (key, rel) in index.entries() {
            let Some(rest) = rel.strip_prefix(prefix.as_str()) else { continue };
            if rest.is_empty() {
                continue;
            }
            let revs: Vec<Revision> =
                log::read(&self.dir, key).into_iter().filter(|r| r.at <= cutoff).collect();
            let Some(last) = revs.last() else { continue };
            let gone = !last.kind.has_content();

            match rest.split_once('/') {
                None => files.push(FolderEntry {
                    path: rel.to_string(),
                    name: rest.to_string(),
                    is_dir: false,
                    deleted: gone,
                    at: last.at,
                    revisions: revs.len(),
                }),
                Some((child, _)) => {
                    let e = dirs.entry(child.to_string()).or_insert((0, 0, true));
                    e.0 = e.0.max(last.at);
                    e.1 += revs.len();
                    // A directory is gone only when everything the history knows under
                    // it is gone. One surviving file means the directory survived.
                    e.2 &= gone;
                }
            }
        }

        let mut out: Vec<FolderEntry> = dirs
            .into_iter()
            .map(|(name, (at, revisions, deleted))| FolderEntry {
                path: format!("{prefix}{name}"),
                name,
                is_dir: true,
                deleted,
                at,
                revisions,
            })
            .collect();
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out.extend(files);
        Ok(out)
    }

    /// The timeline of a directory (or the whole project, for the root): one row per
    /// operation, with the files it touched.
    ///
    /// Grouped by change set where there is one, and by the (moment, kind) pair where
    /// there is not — which is what makes a six-file refactor one row instead of six,
    /// without every caller having to invent a change id for an ordinary save.
    pub fn timeline(&self, dir: &Path, limit: usize) -> HistoryResult<Vec<ChangeGroup>> {
        let base = self.rel(dir)?;
        let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
        let index = Index::load(&self.dir);

        let mut groups: BTreeMap<String, ChangeGroup> = BTreeMap::new();
        for (key, rel) in index.entries() {
            if !rel.starts_with(prefix.as_str()) {
                continue;
            }
            for r in log::read(&self.dir, key) {
                let gid = r.change.clone().unwrap_or_else(|| format!("{}:{:?}", r.at, r.kind));
                let g = groups.entry(gid.clone()).or_insert_with(|| ChangeGroup {
                    id: gid,
                    at: r.at,
                    kind: r.kind,
                    title: r.title.clone(),
                    label: None,
                    files: Vec::new(),
                });
                g.at = g.at.max(r.at);
                if r.label.is_some() {
                    g.label = r.label.clone();
                }
                g.files.push(ChangeFile { path: rel.to_string(), revision: r.id, kind: r.kind });
            }
        }
        let mut out: Vec<ChangeGroup> = groups.into_values().collect();
        out.sort_by(|a, b| b.at.cmp(&a.at));
        out.truncate(limit);
        Ok(out)
    }

    /// The files a change set **deleted**, project-relative.
    ///
    /// The half of a change set an undo cares about: a delete of six files is one
    /// operation, so putting it back has to be one operation too, and the only thing
    /// tying the six together is the change id they share.
    pub fn deleted_in_change(&self, change: &str) -> Vec<String> {
        let index = Index::load(&self.dir);
        index
            .entries()
            .filter(|(key, _)| {
                log::read(&self.dir, key)
                    .iter()
                    .any(|r| !r.kind.has_content() && r.change.as_deref() == Some(change))
            })
            .map(|(_, rel)| rel.to_string())
            .collect()
    }

    // ── housekeeping ─────────────────────────────────────────────────────────

    pub fn usage(&self) -> Usage {
        let index = Index::load(&self.dir);
        let revisions = index.entries().map(|(k, _)| log::read(&self.dir, k).len()).sum();
        Usage { files: index.len(), revisions, bytes: blobs::total_bytes(&self.dir) }
    }

    /// Apply the retention policy: drop what is too old, then what does not fit, then
    /// the blobs nothing points at any more.
    ///
    /// Two things survive both passes unconditionally, and they are what keeps the
    /// policy from being a slow way of losing everything: a **labelled** revision, and
    /// each file's **newest** one. Without the second, a file untouched for eight days
    /// would quietly stop having a history at all — and it would do so exactly when its
    /// history is the only copy left of what it used to be.
    pub fn purge(&self) -> HistoryResult<PurgeReport> {
        let mut report = PurgeReport::default();
        let index = Index::load(&self.dir);
        let horizon = now_ms() - (self.cfg.keep_days as i64) * 86_400_000;

        let mut kept: Vec<(String, Vec<Revision>)> = Vec::new();
        for (key, _) in index.entries() {
            let revs = log::read(&self.dir, key);
            let newest = revs.last().map(|r| r.id.clone());
            let after: Vec<Revision> = revs
                .iter()
                .filter(|r| {
                    r.label.is_some() || r.at >= horizon || Some(&r.id) == newest.as_ref()
                })
                .cloned()
                .collect();
            report.revisions_dropped += revs.len() - after.len();
            if after.len() != revs.len() {
                log::rewrite(&self.dir, key, &after)?;
            }
            kept.push((key.to_string(), after));
        }

        // Over budget: drop oldest-first across the whole project, same two exemptions.
        let mut total = blobs::total_bytes(&self.dir);
        if total > self.cfg.max_bytes {
            let mut candidates: Vec<(i64, usize, usize)> = Vec::new(); // (at, file idx, rev idx)
            for (fi, (_, revs)) in kept.iter().enumerate() {
                for (ri, r) in revs.iter().enumerate() {
                    let newest = ri + 1 == revs.len();
                    if r.label.is_none() && !newest {
                        candidates.push((r.at, fi, ri));
                    }
                }
            }
            candidates.sort_by_key(|c| c.0);
            let mut drop: HashSet<(usize, usize)> = HashSet::new();
            for (_, fi, ri) in candidates {
                if total <= self.cfg.max_bytes {
                    break;
                }
                total = total.saturating_sub(kept[fi].1[ri].size);
                drop.insert((fi, ri));
            }
            for (fi, (key, revs)) in kept.iter_mut().enumerate() {
                if !(0..revs.len()).any(|ri| drop.contains(&(fi, ri))) {
                    continue;
                }
                let after: Vec<Revision> = revs
                    .iter()
                    .enumerate()
                    .filter(|(ri, _)| !drop.contains(&(fi, *ri)))
                    .map(|(_, r)| r.clone())
                    .collect();
                report.revisions_dropped += revs.len() - after.len();
                log::rewrite(&self.dir, key, &after)?;
                *revs = after;
            }
        }

        // Unreferenced blobs. Done last and by set difference rather than by reference
        // counting: a count that drifts deletes content that is still pointed at, and
        // there is no way to notice until somebody asks for it.
        let live: HashSet<String> =
            kept.iter().flat_map(|(_, r)| r.iter().filter_map(|r| r.blob.clone())).collect();
        for h in blobs::all(&self.dir) {
            if !live.contains(&h) {
                report.bytes_freed += blobs::remove(&self.dir, &h);
                report.blobs_dropped += 1;
            }
        }
        Ok(report)
    }

    /// Delete everything. The user asked; there is nothing clever to do about it.
    pub fn clear(&self) -> HistoryResult<()> {
        for sub in ["log", "blobs"] {
            let _ = std::fs::remove_dir_all(self.dir.join(sub));
        }
        let _ = std::fs::remove_file(self.dir.join("paths.jsonl"));
        Ok(())
    }
}

/// Forward slashes, no trailing separator — the one spelling of a path this crate uses,
/// because a store keyed by a string cannot have two spellings of the same project.
fn norm(p: &Path) -> String {
    p.display().to_string().replace('\\', "/").trim_end_matches('/').to_string()
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project root plus a store directory, both cleaned up on drop, so these tests
    /// need no dev-dependency.
    struct Fixture {
        data: PathBuf,
        project: PathBuf,
        store: HistoryStore,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            Self::with(tag, HistoryConfig::default())
        }

        fn with(tag: &str, cfg: HistoryConfig) -> Self {
            let base = std::env::temp_dir()
                .join(format!("arbor-hist-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            let data = base.join("data");
            let project = base.join("proj");
            std::fs::create_dir_all(&data).unwrap();
            std::fs::create_dir_all(&project).unwrap();
            let store = HistoryStore::open(&data, &project, cfg).unwrap();
            Self { data, project, store }
        }

        fn file(&self, rel: &str) -> PathBuf {
            let p = self.project.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            p
        }

        fn save(&self, rel: &str, body: &str) -> Option<Revision> {
            self.store
                .record(&self.file(rel), RevisionKind::Saved, Some(body.as_bytes()), &RecordCtx::default())
                .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.data.parent().unwrap());
        }
    }

    #[test]
    fn saving_the_same_bytes_again_is_not_a_revision() {
        let f = Fixture::new("dedup");
        assert!(f.save("src/a.rs", "one").is_some());
        assert!(f.save("src/a.rs", "one").is_none(), "unchanged content adds no row");
        assert!(f.save("src/a.rs", "two").is_some());
        assert_eq!(f.store.history(&f.file("src/a.rs")).unwrap().revisions.len(), 2);
    }

    #[test]
    fn history_comes_back_newest_first() {
        let f = Fixture::new("order");
        f.save("a.rs", "1");
        f.save("a.rs", "2");
        f.save("a.rs", "3");
        let h = f.store.history(&f.file("a.rs")).unwrap();
        let bodies: Vec<String> = h
            .revisions
            .iter()
            .map(|r| String::from_utf8(f.store.content(&f.file("a.rs"), &r.id).unwrap()).unwrap())
            .collect();
        assert_eq!(bodies, vec!["3", "2", "1"]);
    }

    #[test]
    fn a_deleted_file_keeps_a_readable_history() {
        let f = Fixture::new("deleted");
        f.save("src/gone.rs", "content worth keeping");
        f.store
            .record(&f.file("src/gone.rs"), RevisionKind::Deleted, None, &RecordCtx::default())
            .unwrap();

        let list = f.store.deleted();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "gone.rs");
        assert_eq!(list[0].path, "src/gone.rs");
        // The point of the whole list: what it names can be put back.
        let bytes = f.store.last_content(&list[0].path).unwrap();
        assert_eq!(bytes, b"content worth keeping");
        assert!(f.store.history(&f.file("src/gone.rs")).unwrap().deleted);
    }

    #[test]
    fn a_file_that_comes_back_leaves_the_deleted_list() {
        let f = Fixture::new("revived");
        f.save("a.rs", "x");
        f.store.record(&f.file("a.rs"), RevisionKind::Deleted, None, &RecordCtx::default()).unwrap();
        assert_eq!(f.store.deleted().len(), 1);
        // Same bytes as before the delete: still news, because the file exists again.
        assert!(f.save("a.rs", "x").is_some());
        assert!(f.store.deleted().is_empty());
    }

    #[test]
    fn a_folder_folds_its_subdirectories_into_one_row_each() {
        let f = Fixture::new("folder");
        f.save("src/a.rs", "a");
        f.save("src/deep/b.rs", "b");
        f.save("src/deep/c.rs", "c");

        let entries = f.store.folder(&f.project.join("src"), None).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["deep", "a.rs"], "directories first, then files");
        let deep = &entries[0];
        assert!(deep.is_dir);
        assert_eq!(deep.revisions, 2);
        assert!(!deep.deleted);
    }

    #[test]
    fn a_folder_is_gone_only_when_everything_under_it_is() {
        let f = Fixture::new("folder-del");
        f.save("src/deep/b.rs", "b");
        f.save("src/deep/c.rs", "c");
        f.store
            .record(&f.file("src/deep/b.rs"), RevisionKind::Deleted, None, &RecordCtx::default())
            .unwrap();
        let entries = f.store.folder(&f.project.join("src"), None).unwrap();
        assert!(!entries[0].deleted, "one surviving file keeps the directory alive");

        f.store
            .record(&f.file("src/deep/c.rs"), RevisionKind::Deleted, None, &RecordCtx::default())
            .unwrap();
        let entries = f.store.folder(&f.project.join("src"), None).unwrap();
        assert!(entries[0].deleted);
    }

    #[test]
    fn one_operation_over_six_files_is_one_timeline_row() {
        let f = Fixture::new("timeline");
        let ctx = RecordCtx::titled("chg-1", "Rename frame_at → frame_at_ms");
        for name in ["a.rs", "b.rs", "c.rs"] {
            f.store
                .record(&f.file(name), RevisionKind::Refactored, Some(b"old"), &ctx)
                .unwrap();
        }
        f.save("d.rs", "unrelated");

        let rows = f.store.timeline(&f.project, 50).unwrap();
        let refactor = rows.iter().find(|g| g.id == "chg-1").expect("the change set is one row");
        assert_eq!(refactor.files.len(), 3);
        assert_eq!(refactor.title.as_deref(), Some("Rename frame_at → frame_at_ms"));
        assert!(rows.iter().any(|g| g.files.iter().any(|fl| fl.path == "d.rs")));
    }

    #[test]
    fn a_delete_of_several_files_is_one_thing_to_undo() {
        let f = Fixture::new("undelete");
        let ctx = RecordCtx { change: Some("del-1".into()), title: Some("Deleted 3 files".into()), from: None };
        for name in ["a.rs", "b.rs", "c.rs"] {
            f.save(name, "content");
            f.store.record(&f.file(name), RevisionKind::Deleted, None, &ctx).unwrap();
        }
        // A save that is not part of it must not be swept up by the undo.
        f.save("d.rs", "still here");

        let mut files = f.store.deleted_in_change("del-1");
        files.sort();
        assert_eq!(files, vec!["a.rs", "b.rs", "c.rs"]);
        assert_eq!(f.store.last_content("a.rs").unwrap(), b"content");
    }

    #[test]
    fn a_path_outside_the_project_is_refused() {
        let f = Fixture::new("outside");
        let outside = f.project.parent().unwrap().join("elsewhere.rs");
        assert!(matches!(
            f.store.record(&outside, RevisionKind::Saved, Some(b"x"), &RecordCtx::default()),
            Err(HistoryError::Outside(_))
        ));
    }

    #[test]
    fn a_file_over_the_ceiling_is_not_recorded() {
        let f = Fixture::with("big", HistoryConfig { max_file_bytes: 8, ..Default::default() });
        assert!(f.save("small.rs", "1234").is_some());
        assert!(f.save("big.bin", "0123456789").is_none());
    }

    #[test]
    fn purge_keeps_labels_and_the_newest_revision() {
        let f = Fixture::with("purge", HistoryConfig { keep_days: 0, ..Default::default() });
        let first = f.save("a.rs", "one").unwrap();
        f.save("a.rs", "two");
        f.save("a.rs", "three");
        f.store.label(&f.file("a.rs"), &first.id, "prima di rompere tutto").unwrap();

        // `keep_days: 0` puts the horizon at now, so everything is old enough to go.
        let report = f.store.purge().unwrap();
        assert_eq!(report.revisions_dropped, 1, "only the middle one had nothing protecting it");

        let ids: Vec<String> =
            f.store.history(&f.file("a.rs")).unwrap().revisions.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&first.id), "a labelled revision never expires");
        // And the content the survivors point at is still readable — the blob GC must
        // not have taken it with the revision it dropped.
        assert_eq!(f.store.content(&f.file("a.rs"), &first.id).unwrap(), b"one");
    }

    #[test]
    fn purge_collects_the_blobs_nothing_points_at() {
        let f = Fixture::with("gc", HistoryConfig { keep_days: 0, ..Default::default() });
        f.save("a.rs", "one");
        f.save("a.rs", "two");
        f.save("a.rs", "three");
        assert_eq!(crate::blobs::all(&f.store.dir).len(), 3);
        f.store.purge().unwrap();
        assert_eq!(crate::blobs::all(&f.store.dir).len(), 1, "only the newest survives");
    }

    #[test]
    fn disabled_records_nothing() {
        let f = Fixture::with("off", HistoryConfig { enabled: false, ..Default::default() });
        assert!(f.save("a.rs", "x").is_none());
        assert!(f.store.history(&f.file("a.rs")).unwrap().revisions.is_empty());
    }
}
