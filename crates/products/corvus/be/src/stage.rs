//! `stage` domain — git-index + commit handlers, served **out-of-process** by
//! corvus-be.
//!
//! The context is [`CorvusState`]: the repo is opened by the shell-pushed path
//! ([`crate::repo::open`]). The index mutations themselves live in
//! [`corvus_git::stage`] — one verb per operation, each taking a **list of
//! paths**, so "Stage File" and "Stage Folder" are the same code with a
//! different list length and cannot drift apart. What stays here is the glue
//! that needs shell-pushed state: the repo handle, the recovery-snapshot
//! policy, and the plugin hooks.
//!
//! The `revparse_single("HEAD")` peel-bug workarounds, the initial-commit
//! branches, the `0x3000 >> 12` stage extraction and the
//! `cleanup_state()`-only-when-clean discipline are preserved exactly. Error
//! strings are the wire contract: a git2 error Displays as `"Git error: {e}"`,
//! a missing commit as `"Commit not found: {oid}"`, and anything else bare.
//!
//! **Hooks fire here, in-process to this backend** (plugin-relocation Wave 0).
//! `commit` fires the vetoable `corvus:pre_commit` *before* opening/mutating the
//! repo (a non-empty plugin return aborts with `"Commit blocked by
//! plugin:\n{reason}"`) and the fire-and-forget `corvus:commit` *after* the repo
//! handle is dropped — same lock-then-fire discipline and payload keys as the
//! shell's in-process copy.
//!
//! The discard safety snapshots use the shell-pushed recovery policy
//! (`crate::repo::snapshot_policy`), falling back to the built-in default when
//! none was pushed — same configured limits as in-process (W0b).

use corvus_core::prelude::{hooks, CorvusState};
use git2::Status;
use serde_json::json;

use crate::repo::{git, open, snapshot_policy};

// ---------------------------------------------------------------------------
// Commit — fires the vetoable `corvus:pre_commit` + `corvus:commit` hooks inline.
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn commit(state: &CorvusState, tab_id: String, message: String, amend: bool) -> Result<String, String> {
    // ── Pre-commit veto ────────────────────────────────────────────────
    // Plugins subscribed to `corvus:pre_commit` may reject the commit by
    // returning a non-empty string from their handler. The dispatcher
    // short-circuits at the first plugin that vetoes and hands back a
    // `"<plugin>: <reason>"` string, which we surface to the user.
    if let Some(reason) = state.fire_pre_commit_veto(json!({
        "tab_id":  &tab_id,
        "message": &message,
        "amend":   amend,
    })) {
        return Err(format!("Commit blocked by plugin:\n{reason}"));
    }

    // Scope the repo handle so it is dropped before firing plugin hooks
    // (Lua hooks may call git operations which would deadlock if held).
    let oid = {
        let r = open(state, &tab_id)?;

        let sig = r.signature().map_err(|e| format!("Git error: {e}"))?;
        let mut index = r.index().map_err(|e| format!("Git error: {e}"))?;
        let tree_oid = index.write_tree().map_err(|e| format!("Git error: {e}"))?;
        let tree = r.find_tree(tree_oid).map_err(|e| format!("Git error: {e}"))?;

        if amend {
            // Use find_commit(revparse id) to avoid the peel_to_commit libgit2 bug.
            let head_oid = r.revparse_single("HEAD")
                .map_err(|_| "amend failed: no HEAD commit found".to_string())?
                .id();
            let head_commit = r.find_commit(head_oid).map_err(|e| format!("Git error: {e}"))?;
            let oid = head_commit.amend(
                Some("HEAD"),
                Some(&sig),
                Some(&sig),
                None,
                Some(&message),
                Some(&tree),
            ).map_err(|e| format!("Git error: {e}"))?;
            oid.to_string()
        } else {
            let parent_commits: Vec<git2::Commit<'_>> = match r.revparse_single("HEAD") {
                Ok(obj) => vec![r.find_commit(obj.id()).map_err(|e| format!("Git error: {e}"))?],
                Err(_) => vec![], // initial commit — no parent
            };
            let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
            let oid = r.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
                .map_err(|e| format!("Git error: {e}"))?;
            oid.to_string()
        }
    }; // repo handle dropped here

    state.fire_hook(
        hooks::COMMIT,
        json!({
            "tab_id":  &tab_id,
            "oid":     &oid,
            "message": &message,
            "amend":   amend,
        }),
    );
    Ok(oid)
}

// ---------------------------------------------------------------------------
// Stage / Unstage
// ---------------------------------------------------------------------------

/// Stage a list of paths in ONE index write — the single verb behind both
/// "Stage File" and "Stage Folder", because a single file is the one-element
/// case. Fanning out N single-path RPCs would race on `.git/index`: each opens
/// its own handle, rewrites the whole index, and the last writer wins.
///
/// A **rename is two paths** and the caller sends both halves — see
/// [`corvus_git::stage`] for why, and for why this is `git add -A -- <paths>`
/// rather than a hand-rolled add-or-remove.
#[arbor_rpc::handler]
fn stage_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    corvus_git::stage::stage_paths(&r, &paths).map_err(|e| e.to_string())
}

/// Reset a list of paths back to HEAD in one pass — the single verb behind both
/// "Unstage File" and "Unstage Folder".
#[arbor_rpc::handler]
fn unstage_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    corvus_git::stage::unstage_paths(&r, &paths).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn stage_all(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    corvus_git::stage::stage_all(&r).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn unstage_all(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    corvus_git::stage::unstage_all(&r).map_err(|e| e.to_string())
}

/// Throw away the working-tree changes of a list of paths: ONE recovery
/// snapshot for the group (not one per file), then the mutation. The snapshot
/// is this layer's job because the retention policy is shell-pushed config.
#[arbor_rpc::handler]
fn discard_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let r = open(state, &tab_id)?;

    let policy = snapshot_policy(state);
    let _ = corvus_git::recovery::snapshot_with_policy(
        &git(state),
        &r,
        corvus_git::recovery::RecoveryKind::Discard,
        format!("discard {} file(s)", paths.len()),
        &policy,
    );

    corvus_git::stage::discard_paths(&r, &paths).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn discard_all(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    // Safety net: snapshot the workdir before nuking it — the user can recover
    // via the Recovery tab if "Discard all" was the wrong button to click.
    let policy = snapshot_policy(state);
    let _ = corvus_git::recovery::snapshot_with_policy(
        &git(state),
        &r,
        corvus_git::recovery::RecoveryKind::Discard,
        "discard all changes".to_string(),
        &policy,
    );

    // Collect untracked files/dirs before checkout so we can delete them.
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = r.statuses(Some(&mut status_opts)).map_err(|e| format!("Git error: {e}"))?;

    let workdir = r.workdir()
        .ok_or_else(|| "bare repository".to_string())?
        .to_path_buf();

    for entry in statuses.iter() {
        if entry.status().intersects(Status::WT_NEW) {
            if let Some(p) = entry.path() {
                let abs = workdir.join(p);
                if abs.is_dir() {
                    let _ = std::fs::remove_dir_all(&abs);
                } else if abs.exists() {
                    let _ = std::fs::remove_file(&abs);
                }
            }
        }
    }

    // Restore all tracked modifications from the index.
    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    checkout_opts.force();
    r.checkout_index(None, Some(&mut checkout_opts))
        .map_err(|e| format!("Git error: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Partial staging — apply a hand-crafted unified-diff patch to the index
// ---------------------------------------------------------------------------

/// Apply a unified diff patch to the repository index.
/// Used for line-level / hunk-level staging and unstaging:
/// the frontend builds the exact patch text and this command applies it.
#[arbor_rpc::handler]
fn stage_patch(state: &CorvusState, tab_id: String, patch: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    let diff = git2::Diff::from_buffer(patch.as_bytes())
        .map_err(|e| format!("invalid patch: {e}"))?;

    r.apply(&diff, git2::ApplyLocation::Index, None)
        .map_err(|e| format!("patch apply failed: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Cherry-pick / Revert (via libgit2)
// ---------------------------------------------------------------------------

/// Result of a cherry-pick or revert operation.
/// libgit2 returns success even when the operation produced merge conflicts
/// (it writes conflict markers and sets CHERRY_PICK_HEAD / REVERT_HEAD).
/// This struct lets the frontend distinguish a clean apply from one that
/// requires conflict resolution before committing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CherryPickResult {
    pub has_conflicts: bool,
    /// Relative paths of files with unresolved conflicts (stage > 0 in the index).
    pub conflicted_files: Vec<String>,
    /// True when the cherry-pick / revert produced no diff against HEAD —
    /// typically because the commit's changes are already present in the
    /// current branch.  The UI shows a distinct "no changes" message
    /// instead of the usual success toast.
    #[serde(default)]
    pub no_changes: bool,
}

/// Read conflict state from the index (stage > 0 entries).
fn read_conflicts(repo: &git2::Repository) -> Result<Vec<String>, String> {
    let mut index = repo.index().map_err(|e| format!("Git error: {e}"))?;
    // Force re-read so we see the state just written by cherrypick/revert.
    index.read(true).map_err(|e| format!("Git error: {e}"))?;
    let mut seen = std::collections::HashSet::new();
    for entry in index.iter() {
        // Bits [12:13] in the flags field encode the stage number.
        let stage = (entry.flags & 0x3000) >> 12;
        if stage > 0 {
            seen.insert(String::from_utf8_lossy(&entry.path).into_owned());
        }
    }
    Ok(seen.into_iter().collect())
}

#[arbor_rpc::handler]
fn cherry_pick(state: &CorvusState, tab_id: String, oid: String) -> Result<CherryPickResult, String> {
    let r = open(state, &tab_id)?;
    let git_oid = git2::Oid::from_str(&oid)
        .map_err(|_| format!("Commit not found: {oid}"))?;
    let commit = r.find_commit(git_oid).map_err(|e| format!("Git error: {e}"))?;
    // Merge commits expose two diffs (one per parent); libgit2 needs
    // `mainline` to pick which side to apply. Default to parent 1 — the same
    // choice as `git cherry-pick -m 1` — so cherry-picking a merge replays
    // the work that was merged in.
    let mut opts = git2::CherrypickOptions::new();
    if commit.parent_count() > 1 {
        opts.mainline(1);
    }
    r.cherrypick(&commit, Some(&mut opts)).map_err(|e| format!("Git error: {e}"))?;
    let conflicted_files = read_conflicts(&r)?;

    // On a clean apply libgit2 still leaves CHERRY_PICK_HEAD behind — that
    // blocks every subsequent pull / merge / rebase with "you have not
    // concluded your cherry-pick".  We do NOT auto-commit: the user wants
    // to inspect and commit the staged changes themselves (or amend / squash
    // / drop them) via the Stage area.  Just clear the sentinel so the repo
    // is no longer "stuck" in cherry-pick mode.
    let mut no_changes = false;
    if conflicted_files.is_empty() {
        no_changes = index_matches_head(&r)?;
        r.cleanup_state().map_err(|e| format!("Git error: {e}"))?;
    }

    Ok(CherryPickResult {
        has_conflicts: !conflicted_files.is_empty(),
        conflicted_files,
        no_changes,
    })
}

/// True when the working index is identical to HEAD's tree — i.e. the
/// cherry-pick / revert produced no net diff.  Happens when the commit's
/// changes are already present on the current branch.
fn index_matches_head(repo: &git2::Repository) -> Result<bool, String> {
    let mut index = repo.index().map_err(|e| format!("Git error: {e}"))?;
    // Refresh so the comparison sees writes just produced by cherrypick().
    index.read(true).map_err(|e| format!("Git error: {e}"))?;
    let index_tree_oid = index.write_tree().map_err(|e| format!("Git error: {e}"))?;

    let head_obj    = repo.revparse_single("HEAD").map_err(|e| format!("Git error: {e}"))?;
    let head_commit = repo.find_commit(head_obj.id()).map_err(|e| format!("Git error: {e}"))?;
    let head_tree   = head_commit.tree().map_err(|e| format!("Git error: {e}"))?;

    Ok(index_tree_oid == head_tree.id())
}

#[arbor_rpc::handler]
fn revert_commit(state: &CorvusState, tab_id: String, oid: String) -> Result<CherryPickResult, String> {
    let r = open(state, &tab_id)?;
    let git_oid = git2::Oid::from_str(&oid)
        .map_err(|_| format!("Commit not found: {oid}"))?;
    let commit = r.find_commit(git_oid).map_err(|e| format!("Git error: {e}"))?;
    // Merge commits have multiple parents, so libgit2 needs to know which
    // parent to treat as the mainline (the side to keep). Default to parent 1
    // — equivalent to `git revert -m 1` — which is what callers want in
    // virtually all cases (undo what was merged in, keep the receiving branch).
    let mut opts = git2::RevertOptions::new();
    if commit.parent_count() > 1 {
        opts.mainline(1);
    }
    r.revert(&commit, Some(&mut opts)).map_err(|e| format!("Git error: {e}"))?;
    let conflicted_files = read_conflicts(&r)?;

    // libgit2 leaves REVERT_HEAD behind even on a clean apply — same trap as
    // cherry-pick. Clear it so the repo isn't stuck in "revert mode" after
    // the user commits via the Stage area (git2's commit API doesn't know
    // about REVERT_HEAD; only the `git` CLI clears it implicitly).
    let mut no_changes = false;
    if conflicted_files.is_empty() {
        no_changes = index_matches_head(&r)?;
        r.cleanup_state().map_err(|e| format!("Git error: {e}"))?;
    }

    Ok(CherryPickResult {
        has_conflicts: !conflicted_files.is_empty(),
        conflicted_files,
        no_changes,
    })
}

// ---------------------------------------------------------------------------
// Commit template
// ---------------------------------------------------------------------------

/// Read the commit template from git's `commit.template` config entry (if set).
/// Returns `None` if no template is configured or the file cannot be read.
#[arbor_rpc::handler]
fn get_git_commit_template(state: &CorvusState, tab_id: String) -> Result<Option<String>, String> {
    let repo = open(state, &tab_id)?;

    let config = match repo.config() {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let template_path = match config.get_string("commit.template") {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    // Expand leading ~ to the home directory.
    let path = if template_path.starts_with('~') {
        match dirs::home_dir() {
            Some(home) => home.join(template_path.trim_start_matches("~/").trim_start_matches("~\\")),
            None => std::path::PathBuf::from(&template_path),
        }
    } else {
        // Relative paths are resolved against the repo root.
        let repo_path = repo.workdir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        if std::path::Path::new(&template_path).is_absolute() {
            std::path::PathBuf::from(&template_path)
        } else {
            repo_path.join(&template_path)
        }
    };

    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(_) => Ok(None),
    }
}
