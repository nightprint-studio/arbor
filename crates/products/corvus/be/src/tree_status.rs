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
use serde::Serialize;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conflict_outranks_everything_and_untracked_ranks_last() {
        assert!(rank(GitBadge::Conflicted) > rank(GitBadge::Modified));
        assert!(rank(GitBadge::Modified) > rank(GitBadge::Added));
        assert!(rank(GitBadge::Added) > rank(GitBadge::Untracked));
    }
}
