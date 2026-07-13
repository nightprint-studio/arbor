//! `stage` domain — git-index + commit handlers, served **out-of-process** by
//! corvus-be.
//!
//! Same handler set (and function names → method names) as the shell's
//! in-process copy (`crate::ipc::corvus::stage`), but the context is
//! [`CorvusState`] instead of the shell's `AppState`: the repo is opened by the
//! shell-pushed path ([`crate::repo::open`]). The git work here is **direct
//! libgit2** (not `corvus-git` CLI wrappers), ported byte-for-byte from the
//! in-process source — the `revparse_single("HEAD")` peel-bug workarounds, the
//! initial-commit branches, the `0x3000 >> 12` stage extraction, and the
//! `cleanup_state()`-only-when-clean discipline are all preserved exactly. The
//! error strings match in-process: a git2 error surfaced via the old
//! `AppError::Git` Displays as `"Git error: {e}"`, `AppError::CommitNotFound`
//! as `"Commit not found: {oid}"`, and `AppError::Other(s)` as the bare string.
//!
//! **Hooks fire here, in-process to this backend** (plugin-relocation Wave 0).
//! `commit` fires the vetoable `on_pre_commit` *before* opening/mutating the
//! repo (a non-empty plugin return aborts with `"Commit blocked by
//! plugin:\n{reason}"`) and the fire-and-forget `on_commit` *after* the repo
//! handle is dropped — same lock-then-fire discipline and payload keys as the
//! shell's in-process copy.
//!
//! The discard safety snapshots use the shell-pushed recovery policy
//! (`crate::repo::snapshot_policy`), falling back to the built-in default when
//! none was pushed — same configured limits as in-process (W0b).

use corvus_core::prelude::CorvusState;
use git2::{IndexAddOption, Status};
use serde_json::json;

use crate::repo::{git, open, snapshot_policy};

// ---------------------------------------------------------------------------
// Commit — fires the vetoable `on_pre_commit` + `on_commit` hooks inline.
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn commit(state: &CorvusState, tab_id: String, message: String, amend: bool) -> Result<String, String> {
    // ── Pre-commit veto ────────────────────────────────────────────────
    // Plugins subscribed to `on_pre_commit` may reject the commit by
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
        "on_commit",
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

#[arbor_rpc::handler]
fn stage_file(state: &CorvusState, tab_id: String, path: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    // libgit2's `index.add_path` calls `stat` on the workdir file, which
    // fails (ENOENT / NotFound) when the file has been deleted — the very
    // case the user is trying to stage.  The git CLI handles this by
    // detecting a deletion and calling the equivalent of `remove_path`.
    // Mirror that behaviour: if the file isn't on disk, stage the removal.
    let exists_on_disk = r.workdir()
        .map(|w| w.join(&path).exists())
        .unwrap_or(false);

    let mut index = r.index().map_err(|e| format!("Git error: {e}"))?;
    let p = std::path::Path::new(&path);
    if exists_on_disk {
        index.add_path(p).map_err(|e| format!("Git error: {e}"))?;
    } else {
        index.remove_path(p).map_err(|e| format!("Git error: {e}"))?;
    }
    index.write().map_err(|e| format!("Git error: {e}"))?;
    Ok(())
}

#[arbor_rpc::handler]
fn unstage_file(state: &CorvusState, tab_id: String, path: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    // revparse_single("HEAD") resolves HEAD directly to a commit Object via the
    // rev-parse engine — it does NOT call git_reference_peel which is the function
    // that triggers the InvalidSpec (-12) bug in vendored libgit2.
    // reset_default() expects a *commit* object (not a tree), so we must NOT use
    // the "HEAD^{tree}" specifier here.
    match r.revparse_single("HEAD") {
        Ok(head_obj) => {
            // Normal path: reset the index entry for this path back to HEAD commit.
            r.reset_default(Some(&head_obj), std::iter::once(path.as_str()))
                .map_err(|e| format!("unstage '{path}': {e}"))?;
        }
        Err(_) => {
            // Initial-commit scenario: HEAD doesn't exist yet, so remove the
            // path from the index directly (equivalent to `git rm --cached`).
            let mut index = r.index().map_err(|e| {
                format!("unstage '{path}': cannot open index: {e}")
            })?;
            index.remove_path(std::path::Path::new(&path)).map_err(|e| {
                format!("unstage '{path}': {e}")
            })?;
            index.write().map_err(|e| {
                format!("unstage '{path}': cannot write index: {e}")
            })?;
        }
    }

    Ok(())
}

#[arbor_rpc::handler]
fn stage_all(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    let mut index = r.index().map_err(|e| format!("Git error: {e}"))?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .map_err(|e| format!("Git error: {e}"))?;
    index.write().map_err(|e| format!("Git error: {e}"))?;
    Ok(())
}

#[arbor_rpc::handler]
fn unstage_all(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    // Use revparse_single to avoid the peel_to_commit libgit2 bug.
    match r.revparse_single("HEAD") {
        Ok(head_obj) => {
            r.reset(&head_obj, git2::ResetType::Mixed, None)
                .map_err(|e| format!("Git error: {e}"))?;
        }
        Err(_) => {
            // Initial commit: clear the index entirely.
            let mut index = r.index().map_err(|e| format!("Git error: {e}"))?;
            index.clear().map_err(|e| format!("Git error: {e}"))?;
            index.write().map_err(|e| format!("Git error: {e}"))?;
        }
    }
    Ok(())
}

/// Stage a whole list of paths **atomically** — open the index ONCE, apply every
/// add/remove, then a single `index.write()`. This is what the "stage folder" action calls:
/// fanning out N concurrent `stage_file` RPCs would race on `.git/index` (each opens its own
/// handle, reads the on-disk index, mutates one entry, and writes the WHOLE index back — the
/// last writer wins, so only a subset of the folder ends up staged, or `index.lock` collides).
/// One handler, one write = no race.
#[arbor_rpc::handler]
fn stage_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    let workdir = r.workdir().map(|w| w.to_path_buf());
    let mut index = r.index().map_err(|e| format!("Git error: {e}"))?;
    for path in &paths {
        // Same deletion handling as `stage_file`: a file missing from the workdir is staged as a
        // removal (libgit2's `add_path` would fail its `stat`).
        let exists_on_disk = workdir.as_ref().map(|w| w.join(path).exists()).unwrap_or(false);
        let p = std::path::Path::new(path);
        if exists_on_disk {
            index.add_path(p).map_err(|e| format!("Git error: {e}"))?;
        } else {
            index.remove_path(p).map_err(|e| format!("Git error: {e}"))?;
        }
    }
    index.write().map_err(|e| format!("Git error: {e}"))?;
    Ok(())
}

/// Unstage a whole list of paths **atomically** (the "unstage folder" action). On a repo with a
/// HEAD, a single `reset_default` resets every pathspec at once; on an initial commit (no HEAD),
/// remove them all from one index handle before a single write. Avoids the same concurrent-index
/// race as [`stage_paths`].
#[arbor_rpc::handler]
fn unstage_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    let r = open(state, &tab_id)?;
    match r.revparse_single("HEAD") {
        Ok(head_obj) => {
            r.reset_default(Some(&head_obj), paths.iter().map(String::as_str))
                .map_err(|e| format!("unstage: {e}"))?;
        }
        Err(_) => {
            // Initial-commit scenario: no HEAD yet → remove each path from the index directly.
            let mut index = r.index().map_err(|e| format!("unstage: cannot open index: {e}"))?;
            for path in &paths {
                index.remove_path(std::path::Path::new(path))
                    .map_err(|e| format!("unstage '{path}': {e}"))?;
            }
            index.write().map_err(|e| format!("unstage: cannot write index: {e}"))?;
        }
    }
    Ok(())
}

#[arbor_rpc::handler]
fn discard_file(state: &CorvusState, tab_id: String, path: String) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    let file_status = r.status_file(std::path::Path::new(&path)).unwrap_or(Status::empty());

    // Safety net: snapshot the workdir before discarding so untracked files
    // and local edits can be recovered from the Recovery tab.
    let policy = snapshot_policy(state);
    let _ = corvus_git::recovery::snapshot_with_policy(
        &git(state),
        &r,
        corvus_git::recovery::RecoveryKind::Discard,
        format!("discard '{path}'"),
        &policy,
    );

    if file_status.intersects(Status::WT_NEW) {
        // Untracked / new file — delete it from the filesystem.
        let abs = r.workdir()
            .ok_or_else(|| "bare repository".to_string())?
            .join(&path);
        if abs.exists() {
            if abs.is_dir() {
                std::fs::remove_dir_all(&abs)
                    .map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_file(&abs)
                    .map_err(|e| e.to_string())?;
            }
        }
    } else {
        // Tracked file — restore from index.
        let mut checkout_opts = git2::build::CheckoutBuilder::new();
        checkout_opts.path(&path).force();
        r.checkout_index(None, Some(&mut checkout_opts))
            .map_err(|e| format!("Git error: {e}"))?;
    }
    Ok(())
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

/// Discard a whole list of paths **atomically** (the "discard folder" action): ONE recovery
/// snapshot for the group, then per-path — delete untracked files, and restore tracked ones via a
/// SINGLE `checkout_index` carrying every tracked pathspec. Fanning out N concurrent `discard_file`
/// RPCs would take N snapshots and race on `checkout_index`; this takes one snapshot and one checkout.
#[arbor_rpc::handler]
fn discard_paths(state: &CorvusState, tab_id: String, paths: Vec<String>) -> Result<(), String> {
    let r = open(state, &tab_id)?;

    // One snapshot for the whole folder (not one per file).
    let policy = snapshot_policy(state);
    let _ = corvus_git::recovery::snapshot_with_policy(
        &git(state),
        &r,
        corvus_git::recovery::RecoveryKind::Discard,
        format!("discard {} file(s)", paths.len()),
        &policy,
    );

    let workdir = r.workdir().ok_or_else(|| "bare repository".to_string())?.to_path_buf();
    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    let mut any_tracked = false;
    for path in &paths {
        let file_status = r.status_file(std::path::Path::new(path)).unwrap_or(Status::empty());
        if file_status.intersects(Status::WT_NEW) {
            // Untracked / new — delete from disk.
            let abs = workdir.join(path);
            if abs.exists() {
                if abs.is_dir() {
                    std::fs::remove_dir_all(&abs).map_err(|e| e.to_string())?;
                } else {
                    std::fs::remove_file(&abs).map_err(|e| e.to_string())?;
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
        r.checkout_index(None, Some(&mut checkout_opts)).map_err(|e| format!("Git error: {e}"))?;
    }
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
