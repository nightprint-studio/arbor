//! Worktrees for an AI assistant's isolated copy of a project — reached by path, with no UI tab.
//!
//! Bennu's assistant can work on a copy of the project so nothing it does touches the real files until the
//! user applies it. For a git repository that copy is a **detached linked worktree** (cheap: no full copy, no
//! branch left behind in the user's repository). These two handlers are the whole of what the assistant is
//! allowed to ask of git for that: add one, remove one. They are reached through the shell's
//! `__assistant_worktree` relay, which names exactly these two methods and nothing else of Corvus.
//!
//! The git work itself is [`corvus_git::worktree`] — the same code the worktree manager uses.

use std::path::Path;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{add_worktree, get_status, remove_worktree, StatusEntry};
use serde::{Deserialize, Serialize};

use crate::ephemeral::tab_for;
use crate::repo::{git, open, repo_path};

#[derive(Debug, Deserialize)]
pub struct AddArgs {
    /// Any folder or file inside the repository.
    pub root: String,
    /// Where the worktree goes. Must not exist yet.
    pub dest: String,
}

/// What `assistant_worktree_add` made.
#[derive(Debug, Serialize)]
pub struct Added {
    /// The repository's working directory (the project may be a subfolder of it).
    pub workdir: String,
    /// The commit the worktree was made at.
    pub head: Option<String>,
    pub branch: Option<String>,
    /// Paths (relative to `workdir`, forward slashes) the user had changed, staged or not, plus untracked
    /// ones that are not ignored: the caller copies these from the working directory so the worktree starts
    /// from what the user sees, not from the last commit.
    pub changed: Vec<String>,
}

fn changed_paths(entries: &[&Vec<StatusEntry>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in entries.iter().flat_map(|v| v.iter()) {
        out.push(e.path.clone());
        // A rename: the old path is gone from the working directory.
        if let Some(old) = &e.old_path {
            out.push(old.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Add a detached linked worktree of the repository containing `root` at `dest`, and say what the
/// working directory has that HEAD does not.
#[arbor_rpc::handler]
fn assistant_worktree_add(state: &CorvusState, args: AddArgs) -> Result<Added, String> {
    let tab = tab_for(state, &args.root)?;
    let workdir = repo_path(state, &tab)?;
    // Forget worktrees whose folder a crash or a sweep removed, so they do not pile up in the repository.
    let _ = git(state).command().args(["worktree", "prune"]).current_dir(&workdir).output();
    let status = {
        let repo = open(state, &tab)?;
        get_status(&repo).map_err(|e| e.to_string())?
    };
    if status.is_rebasing || status.is_merging || status.is_cherry_picking || status.is_reverting {
        return Err("the repository is in the middle of a rebase, merge, cherry-pick or revert: finish or abort it first".into());
    }
    // `HEAD` as the start point, no new branch: a detached checkout that leaves nothing behind in the
    // user's branch list.
    add_worktree(&git(state), Path::new(&workdir), &args.dest, "HEAD", None).map_err(|e| e.to_string())?;
    Ok(Added {
        workdir: workdir.replace('\\', "/"),
        head: status.head_oid.clone(),
        branch: status.current_branch.clone(),
        changed: changed_paths(&[&status.staged, &status.unstaged, &status.untracked, &status.conflicted]),
    })
}

#[derive(Debug, Deserialize)]
pub struct RemoveArgs {
    pub root: String,
    pub dest: String,
}

/// Remove the worktree at `dest` (forced: it is a throwaway) and prune what git remembers of it.
#[arbor_rpc::handler]
fn assistant_worktree_remove(state: &CorvusState, args: RemoveArgs) -> Result<(), String> {
    let tab = tab_for(state, &args.root)?;
    let workdir = repo_path(state, &tab)?;
    let result = remove_worktree(&git(state), Path::new(&workdir), &args.dest).map_err(|e| e.to_string());
    // The directory may already be gone (crash recovery); then only git's record of it is left to clear.
    let _ = git(state).command().args(["worktree", "prune"]).current_dir(&workdir).output();
    result
}
