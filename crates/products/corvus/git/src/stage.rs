//! `stage` domain — index mutations over an explicit **set of paths**.
//!
//! One verb per operation, each taking a list: "stage this file" is the
//! one-element case of "stage these files", so the single-file action and the
//! folder action cannot drift apart. It is also the only safe shape — fanning
//! out N single-path calls would race on `.git/index`, because each one opens
//! its own handle, reads the on-disk index, mutates one entry and writes the
//! WHOLE index back. Last writer wins, so only a subset of the folder lands.
//!
//! ## Staging is `git add -A -- <paths>`
//!
//! [`stage_paths`] goes through `git_index_add_all` with the paths as
//! pathspecs — the same call [`stage_all`] makes with `*`, which is why the two
//! can no longer disagree. libgit2 handles additions, modifications **and
//! deletions** in that one pass and honours `.gitignore`. The hand-rolled
//! predecessor picked `add_path` or `remove_path` by asking whether the file
//! was still on disk: it re-implemented deletion handling libgit2 already does,
//! and `add_path` force-adds an ignored file the way `git add -f` would.
//!
//! ## A rename is two paths
//!
//! git stages a move as the removal of the old path plus the addition of the
//! new one, so **both halves must be in the list**. Callers pass
//! [`crate::status::StatusEntry::old_path`] alongside `path`; send one half
//! only and the other side of the move is silently left behind — which is what
//! made "Stage Folder" look like it was inventing untracked files.

use std::path::Path;

use git2::{IndexAddOption, Repository, Status};

use crate::error::{GitError, Result};

/// Stage every path in `paths` in a single index write.
///
/// Empty list = no-op: a caller with nothing selected should not be an error.
pub fn stage_paths(repo: &Repository, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut index = repo.index()?;
    index.add_all(paths.iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

/// Stage the whole working tree — `git add -A`.
pub fn stage_all(repo: &Repository) -> Result<()> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

/// Reset every path in `paths` back to HEAD in a single pass.
///
/// On a repo with a HEAD one `reset_default` takes every pathspec at once. On
/// an initial commit there is no HEAD to reset to, so the entries are removed
/// from one index handle before a single write (`git rm --cached`).
///
/// Empty list = no-op — and load-bearing: libgit2 rejects an empty pathspec
/// array outright (`'pathspecs && pathspecs->count > 0'`).
pub fn unstage_paths(repo: &Repository, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    match repo.revparse_single("HEAD") {
        // revparse_single resolves HEAD to a commit through the rev-parse
        // engine — it does NOT call git_reference_peel, the function that
        // trips the InvalidSpec (-12) bug in vendored libgit2. reset_default
        // wants a *commit*, so "HEAD^{tree}" would be wrong here.
        Ok(head_obj) => {
            repo.reset_default(Some(&head_obj), paths.iter().map(String::as_str))
                .map_err(|e| GitError::Other(format!("unstage: {e}")))?;
        }
        Err(_) => {
            let mut index = repo
                .index()
                .map_err(|e| GitError::Other(format!("unstage: cannot open index: {e}")))?;
            for path in paths {
                index
                    .remove_path(Path::new(path))
                    .map_err(|e| GitError::Other(format!("unstage '{path}': {e}")))?;
            }
            index
                .write()
                .map_err(|e| GitError::Other(format!("unstage: cannot write index: {e}")))?;
        }
    }
    Ok(())
}

/// Unstage everything — `git reset --mixed HEAD`, or a cleared index on an
/// initial commit.
pub fn unstage_all(repo: &Repository) -> Result<()> {
    match repo.revparse_single("HEAD") {
        Ok(head_obj) => {
            repo.reset(&head_obj, git2::ResetType::Mixed, None)?;
        }
        Err(_) => {
            let mut index = repo.index()?;
            index.clear()?;
            index.write()?;
        }
    }
    Ok(())
}

/// Throw away the working-tree changes of every path in `paths`.
///
/// Untracked files are deleted from disk; tracked ones are restored from the
/// index via a SINGLE `checkout_index` carrying every tracked pathspec. Taking
/// the recovery snapshot is the *caller's* job — it owns the retention policy,
/// and one folder discard must take one snapshot, not one per file.
///
/// Because a rename arrives as both halves, discarding a move deletes the file
/// at its new path and restores it at the old one — the move is undone, which
/// is what "discard" means.
pub fn discard_paths(repo: &Repository, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let workdir = repo
        .workdir()
        .ok_or_else(|| GitError::Other("bare repository".to_string()))?
        .to_path_buf();

    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    let mut any_tracked = false;
    for path in paths {
        let file_status = repo.status_file(Path::new(path)).unwrap_or_else(|_| Status::empty());
        if file_status.intersects(Status::WT_NEW) {
            // Untracked / new — delete it from the filesystem.
            let abs = workdir.join(path);
            if abs.exists() {
                if abs.is_dir() {
                    std::fs::remove_dir_all(&abs).map_err(|e| GitError::Other(e.to_string()))?;
                } else {
                    std::fs::remove_file(&abs).map_err(|e| GitError::Other(e.to_string()))?;
                }
            }
        } else {
            // Tracked — queue it for the single index checkout below.
            checkout_opts.path(path);
            any_tracked = true;
        }
    }
    if any_tracked {
        checkout_opts.force();
        repo.checkout_index(None, Some(&mut checkout_opts))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — real temp repos: the invariants here are libgit2's behaviour, and a
// mock of libgit2 would only assert what we already believe.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{get_status_with, FileStatus};
    use std::path::PathBuf;

    struct TempRepo {
        dir: PathBuf,
        repo: Repository,
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    impl TempRepo {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "corvus-git-stage-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            ));
            let _ = std::fs::remove_dir_all(&dir);
            let repo = Repository::init(&dir).expect("init repo");
            TempRepo { dir, repo }
        }

        fn write(&self, rel: &str, body: &str) {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }

        fn remove(&self, rel: &str) {
            std::fs::remove_file(self.dir.join(rel)).unwrap();
        }

        fn rename(&self, from: &str, to: &str) {
            let dst = self.dir.join(to);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::rename(self.dir.join(from), dst).unwrap();
        }

        fn commit(&self, msg: &str) {
            stage_all(&self.repo).unwrap();
            let mut index = self.repo.index().unwrap();
            let tree = self.repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("t", "t@example.com").unwrap();
            let parents = match self.repo.revparse_single("HEAD") {
                Ok(o) => vec![self.repo.find_commit(o.id()).unwrap()],
                Err(_) => vec![],
            };
            let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
            self.repo
                .commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
                .unwrap();
        }

        /// `(staged, unstaged, untracked)` as `(path, status)` pairs.
        #[allow(clippy::type_complexity)]
        fn status(&self) -> (Vec<(String, FileStatus)>, Vec<(String, FileStatus)>, Vec<String>) {
            let s = get_status_with(&self.repo, true).unwrap();
            let map = |v: &[crate::status::StatusEntry], staged: bool| {
                v.iter()
                    .map(|e| {
                        let st = if staged {
                            e.index_status.clone()
                        } else {
                            e.workdir_status.clone()
                        };
                        (e.path.clone(), st.unwrap())
                    })
                    .collect::<Vec<_>>()
            };
            (
                map(&s.staged, true),
                map(&s.unstaged, false),
                s.untracked.iter().map(|e| e.path.clone()).collect(),
            )
        }

        /// Every path the UI would send for the unstaged pane — entry path plus
        /// the other half of a rename.
        fn unstaged_paths(&self) -> Vec<String> {
            let s = get_status_with(&self.repo, true).unwrap();
            let mut out = Vec::new();
            for e in s.unstaged.iter().chain(s.untracked.iter()) {
                out.push(e.path.clone());
                if let Some(old) = &e.old_path {
                    if old != &e.path {
                        out.push(old.clone());
                    }
                }
            }
            out
        }

        fn staged_paths(&self) -> Vec<String> {
            let s = get_status_with(&self.repo, true).unwrap();
            let mut out = Vec::new();
            for e in &s.staged {
                out.push(e.path.clone());
                if let Some(old) = &e.old_path {
                    if old != &e.path {
                        out.push(old.clone());
                    }
                }
            }
            out
        }
    }

    /// A new file and a deleted one, staged together by the folder action: the
    /// deletion is what the old `add_path`-or-`remove_path` split existed to
    /// handle, and `add_all` has to keep handling it.
    #[test]
    fn stages_additions_and_deletions_in_one_pass() {
        let t = TempRepo::new("mixed");
        t.write("d/keep.txt", "k\n");
        t.write("d/gone.txt", "g\n");
        t.commit("init");
        t.remove("d/gone.txt");
        t.write("d/new.txt", "n\n");

        stage_paths(&t.repo, &t.unstaged_paths()).unwrap();

        let (staged, unstaged, untracked) = t.status();
        assert_eq!(
            staged,
            vec![
                ("d/gone.txt".to_string(), FileStatus::Deleted),
                ("d/new.txt".to_string(), FileStatus::Added),
            ]
        );
        assert!(unstaged.is_empty(), "nothing left unstaged: {unstaged:?}");
        assert!(untracked.is_empty(), "nothing left untracked: {untracked:?}");
    }

    /// The bug this module was rewritten for: a moved file is ONE status entry
    /// carrying two paths, and staging it must stage both halves. Staging only
    /// the entry's path left the arrival behind as a mystery untracked file in
    /// a folder the user never touched.
    #[test]
    fn staging_a_rename_moves_both_halves() {
        let t = TempRepo::new("rename");
        t.write("d/moved.txt", &"line\n".repeat(40));
        t.commit("init");
        t.rename("d/moved.txt", "e/renamed.txt");

        let paths = t.unstaged_paths();
        assert!(paths.contains(&"e/renamed.txt".to_string()), "new half: {paths:?}");
        assert!(paths.contains(&"d/moved.txt".to_string()), "old half: {paths:?}");

        stage_paths(&t.repo, &paths).unwrap();

        let (staged, unstaged, untracked) = t.status();
        assert_eq!(staged, vec![("e/renamed.txt".to_string(), FileStatus::Renamed)]);
        assert!(unstaged.is_empty(), "{unstaged:?}");
        assert!(untracked.is_empty(), "arrival left behind: {untracked:?}");
    }

    /// …and unstaging it puts both halves back, rather than resetting the
    /// departure and orphaning the arrival in the index.
    #[test]
    fn unstaging_a_rename_restores_both_halves() {
        let t = TempRepo::new("unrename");
        t.write("d/moved.txt", &"line\n".repeat(40));
        t.commit("init");
        t.rename("d/moved.txt", "e/renamed.txt");
        stage_paths(&t.repo, &t.unstaged_paths()).unwrap();

        unstage_paths(&t.repo, &t.staged_paths()).unwrap();

        let (staged, unstaged, _) = t.status();
        assert!(staged.is_empty(), "{staged:?}");
        assert_eq!(unstaged, vec![("e/renamed.txt".to_string(), FileStatus::Renamed)]);
    }

    /// Round trip on the plain cases, one path at a time — the single-file
    /// action is the one-element case and must land in the same place.
    #[test]
    fn single_path_round_trips_through_the_same_verbs() {
        let t = TempRepo::new("single");
        t.write("a.txt", "a\n");
        t.commit("init");
        t.write("a.txt", "a2\n");

        stage_paths(&t.repo, &["a.txt".to_string()]).unwrap();
        assert_eq!(t.status().0, vec![("a.txt".to_string(), FileStatus::Modified)]);

        unstage_paths(&t.repo, &["a.txt".to_string()]).unwrap();
        assert_eq!(t.status().1, vec![("a.txt".to_string(), FileStatus::Modified)]);
    }

    /// Brackets are legal in a filename and ubiquitous in SvelteKit routes.
    /// Both calls take pathspecs, so a `[id]` segment could have been read as a
    /// character class and matched nothing at all — silently.
    #[test]
    fn bracketed_filenames_are_matched_literally() {
        let t = TempRepo::new("bracket");
        t.write("routes/[id]/+page.svelte", "x\n");
        t.commit("init");
        t.write("routes/[id]/+page.svelte", "y\n");

        let p = vec!["routes/[id]/+page.svelte".to_string()];
        stage_paths(&t.repo, &p).unwrap();
        assert_eq!(t.status().0.len(), 1, "bracketed path did not stage");
        unstage_paths(&t.repo, &p).unwrap();
        assert_eq!(t.status().1.len(), 1, "bracketed path did not unstage");
    }

    /// An ignored file is not staged by name. `add_path` would have force-added
    /// it the way `git add -f` does; `git add -- <path>` does not.
    #[test]
    fn ignored_files_are_not_staged_by_name() {
        let t = TempRepo::new("ignored");
        t.write(".gitignore", "build/\n");
        t.commit("init");
        t.write("build/out.bin", "x\n");

        stage_paths(&t.repo, &["build/out.bin".to_string()]).unwrap();
        assert!(t.status().0.is_empty(), "ignored file was staged");
    }

    /// Before the first commit there is no HEAD to reset to, so unstaging is
    /// `git rm --cached`. This is the path that would panic on `peel_to_commit`.
    #[test]
    fn unstages_on_an_initial_commit_without_head() {
        let t = TempRepo::new("initial");
        t.write("a.txt", "a\n");
        stage_paths(&t.repo, &["a.txt".to_string()]).unwrap();
        assert_eq!(t.status().0.len(), 1);

        unstage_paths(&t.repo, &["a.txt".to_string()]).unwrap();
        assert_eq!(t.status().2, vec!["a.txt".to_string()]);
    }

    /// An empty selection is a no-op everywhere. libgit2 rejects an empty
    /// pathspec array, so `unstage_paths` would otherwise surface an
    /// "invalid argument" toast for doing nothing.
    #[test]
    fn empty_selection_is_a_no_op_not_an_error() {
        let t = TempRepo::new("empty");
        t.write("a.txt", "a\n");
        t.commit("init");
        t.write("a.txt", "a2\n");

        assert!(stage_paths(&t.repo, &[]).is_ok());
        assert!(unstage_paths(&t.repo, &[]).is_ok());
        assert!(discard_paths(&t.repo, &[]).is_ok());
        assert_eq!(t.status().1.len(), 1, "the no-ops changed something");
    }

    /// Discarding a move undoes it: the arrival is deleted, the departure is
    /// restored. Both halves reach the call, so neither is left behind.
    #[test]
    fn discarding_a_rename_undoes_the_move() {
        let t = TempRepo::new("discard-rename");
        t.write("d/moved.txt", &"line\n".repeat(40));
        t.commit("init");
        t.rename("d/moved.txt", "e/renamed.txt");

        discard_paths(&t.repo, &t.unstaged_paths()).unwrap();

        let (staged, unstaged, untracked) = t.status();
        assert!(staged.is_empty() && unstaged.is_empty(), "{staged:?} {unstaged:?}");
        assert!(untracked.is_empty(), "{untracked:?}");
        assert!(t.dir.join("d/moved.txt").exists(), "departure not restored");
        assert!(!t.dir.join("e/renamed.txt").exists(), "arrival not removed");
    }

    /// Discard mixes the two kinds in one call: an untracked file is deleted,
    /// a tracked edit is checked back out — one `checkout_index` for the group.
    #[test]
    fn discards_untracked_and_tracked_together() {
        let t = TempRepo::new("discard-mixed");
        t.write("d/tracked.txt", "orig\n");
        t.commit("init");
        t.write("d/tracked.txt", "edited\n");
        t.write("d/fresh.txt", "f\n");

        discard_paths(&t.repo, &t.unstaged_paths()).unwrap();

        assert_eq!(std::fs::read_to_string(t.dir.join("d/tracked.txt")).unwrap(), "orig\n");
        assert!(!t.dir.join("d/fresh.txt").exists());
    }
}
