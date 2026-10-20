//! Working-tree status of a whole project, named by **path** — what Bennu's file tree colours
//! its rows from.
//!
//! One call returns every changed file of the repository containing `root`, each with the one
//! badge that sums it up, so the tree can mark rows without a request per folder. No tab is
//! involved (the repository is only discovered from the path), and nothing is cached here: the
//! caller decides when a fresh answer is worth a status walk.

use std::collections::HashMap;

use corvus_core::prelude::CorvusState;
use corvus_git::explorer::GitBadge;
use corvus_git::prelude::{RepoStatus, StatusEntry};
use serde::Serialize;

use crate::ephemeral::tab_for;
use crate::repo::open;

/// Every changed file of a repository, keyed by absolute path with forward slashes.
#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct TreeGitStatus {
    /// `root` is inside a git repository. When false the tree shows no git affordance at all.
    pub in_repo: bool,
    pub branch: Option<String>,
    /// Only non-clean paths. An untracked **directory** appears once, as the directory — the
    /// walk does not descend into it, so its files have no entry of their own.
    pub files: HashMap<String, GitBadge>,
}

/// How much a badge matters when one path has several (a file staged and then edited again is
/// both). The strongest wins.
fn rank(b: GitBadge) -> u8 {
    match b {
        GitBadge::Conflicted => 6,
        GitBadge::Deleted => 5,
        GitBadge::Modified => 4,
        GitBadge::Renamed => 3,
        GitBadge::Added => 2,
        GitBadge::Untracked => 1,
        GitBadge::Ignored => 0,
    }
}

/// The working directory of the repository containing `path`, or `None` outside one. Served by
/// Corvus so a window that only owns a project folder (Bennu) can ask without the File
/// Explorer's backend (sitta) having to be running.
#[arbor_rpc::handler]
fn git_repo_root_by_path(_state: &CorvusState, path: String) -> Result<Option<String>, String> {
    Ok(corvus_git::explorer::repo_root(&path))
}

/// Changed files of the repository containing `root`.
#[arbor_rpc::handler]
fn git_tree_status_by_path(_state: &CorvusState, root: String) -> Result<TreeGitStatus, String> {
    // `changes` answers an empty list for a path that is not in a repository; the repo root it
    // reports is what tells the two empty cases apart (a clean repo vs no repo).
    let changes = corvus_git::explorer::changes(&root);
    if changes.repo_root.is_none() {
        return Ok(TreeGitStatus::default());
    }
    let mut files: HashMap<String, GitBadge> = HashMap::new();
    for change in changes.staged.iter().chain(changes.unstaged.iter()) {
        let path = change.path.replace('\\', "/");
        let keep = files.get(&path).map_or(true, |have| rank(change.badge) > rank(*have));
        if keep {
            files.insert(path, change.badge);
        }
    }
    Ok(TreeGitStatus { in_repo: true, branch: changes.branch, files })
}

/// Where a repository stands, in a form small enough for a status bar: the branch with its
/// distance from upstream, and how many files sit in each bucket. Unlike [`TreeGitStatus`] the
/// staged / unstaged / untracked buckets are kept apart — a file staged and edited again counts
/// in both, as it does in a commit dialog.
#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct GitSummary {
    /// `root` is inside a git repository.
    pub in_repo: bool,
    pub branch: Option<String>,
    /// HEAD points at a commit rather than a branch (`branch` is then `None`).
    pub detached: bool,
    pub ahead: usize,
    pub behind: usize,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

fn summarize(status: &RepoStatus) -> GitSummary {
    GitSummary {
        in_repo: true,
        branch: status.current_branch.clone(),
        detached: status.is_detached,
        ahead: status.ahead,
        behind: status.behind,
        staged: status.staged.len(),
        unstaged: status.unstaged.len(),
        untracked: status.untracked.len(),
        conflicted: status.conflicted.len(),
    }
}

/// Branch, ahead/behind and per-bucket counts of the repository containing `root`.
#[arbor_rpc::handler]
fn git_summary_by_path(state: &CorvusState, root: String) -> Result<GitSummary, String> {
    // Not being inside a repository is an answer, not an error: the status bar just hides.
    let Ok(tab) = tab_for(state, &root) else {
        return Ok(GitSummary::default());
    };
    let repo = open(state, &tab)?;
    // Rename detection is quadratic and the bar only needs counts.
    let status = corvus_git::status::get_status_with(&repo, false).map_err(|e| e.to_string())?;
    Ok(summarize(&status))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> StatusEntry {
        StatusEntry { path: path.into(), old_path: None, index_status: None, workdir_status: None }
    }

    #[test]
    fn the_summary_keeps_each_bucket_apart() {
        let status = RepoStatus {
            current_branch: Some("main".into()),
            head_oid: None,
            is_detached: false,
            ahead: 2,
            behind: 1,
            staged: vec![entry("a"), entry("b")],
            unstaged: vec![entry("a")],
            untracked: vec![entry("c")],
            conflicted: vec![],
            is_rebasing: false,
            is_merging: false,
            is_cherry_picking: false,
            is_reverting: false,
        };
        let s = summarize(&status);
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!((s.staged, s.unstaged, s.untracked, s.conflicted), (2, 1, 1, 0));
        assert_eq!(s.branch.as_deref(), Some("main"));
    }

    #[test]
    fn a_conflict_outranks_everything_and_untracked_ranks_last() {
        assert!(rank(GitBadge::Conflicted) > rank(GitBadge::Modified));
        assert!(rank(GitBadge::Modified) > rank(GitBadge::Added));
        assert!(rank(GitBadge::Added) > rank(GitBadge::Untracked));
    }
}
