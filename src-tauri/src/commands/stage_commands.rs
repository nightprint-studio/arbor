use git2::{IndexAddOption, Status};
use tauri::State;

use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Stage / Unstage
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn stage_file(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    // libgit2's `index.add_path` calls `stat` on the workdir file, which
    // fails (ENOENT / NotFound) when the file has been deleted — the very
    // case the user is trying to stage.  The git CLI handles this by
    // detecting a deletion and calling the equivalent of `remove_path`.
    // Mirror that behaviour: if the file isn't on disk, stage the removal.
    let exists_on_disk = r.workdir()
        .map(|w| w.join(&path).exists())
        .unwrap_or(false);

    let mut index = r.index()?;
    let p = std::path::Path::new(&path);
    if exists_on_disk {
        index.add_path(p)?;
    } else {
        index.remove_path(p)?;
    }
    index.write()?;
    Ok(())
}

#[tauri::command]
pub fn unstage_file(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner();

    // revparse_single("HEAD") resolves HEAD directly to a commit Object via the
    // rev-parse engine — it does NOT call git_reference_peel which is the function
    // that triggers the InvalidSpec (-12) bug in vendored libgit2.
    // reset_default() expects a *commit* object (not a tree), so we must NOT use
    // the "HEAD^{tree}" specifier here.
    match r.revparse_single("HEAD") {
        Ok(head_obj) => {
            // Normal path: reset the index entry for this path back to HEAD commit.
            r.reset_default(Some(&head_obj), std::iter::once(path.as_str()))
                .map_err(|e| AppError::Other(format!("unstage '{path}': {e}")))?;
        }
        Err(_) => {
            // Initial-commit scenario: HEAD doesn't exist yet, so remove the
            // path from the index directly (equivalent to `git rm --cached`).
            let mut index = r.index().map_err(|e| {
                AppError::Other(format!("unstage '{path}': cannot open index: {e}"))
            })?;
            index.remove_path(std::path::Path::new(&path)).map_err(|e| {
                AppError::Other(format!("unstage '{path}': {e}"))
            })?;
            index.write().map_err(|e| {
                AppError::Other(format!("unstage '{path}': cannot write index: {e}"))
            })?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn stage_all(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let mut index = repo.inner_mut().index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

#[tauri::command]
pub fn unstage_all(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner();
    // Use revparse_single to avoid the peel_to_commit libgit2 bug.
    match r.revparse_single("HEAD") {
        Ok(head_obj) => {
            r.reset(&head_obj, git2::ResetType::Mixed, None)?;
        }
        Err(_) => {
            // Initial commit: clear the index entirely.
            let mut index = r.index()?;
            index.clear()?;
            index.write()?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn discard_file(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    let file_status = r.status_file(std::path::Path::new(&path)).unwrap_or(Status::empty());

    // Safety net: snapshot the workdir before discarding so untracked files
    // and local edits can be recovered from the Recovery tab.
    crate::git::recovery::try_snapshot(
        r,
        crate::git::recovery::RecoveryKind::Discard,
        format!("discard '{path}'"),
    );

    if file_status.intersects(Status::WT_NEW) {
        // Untracked / new file — delete it from the filesystem.
        let abs = r.workdir()
            .ok_or_else(|| AppError::Other("bare repository".into()))?
            .join(&path);
        if abs.exists() {
            if abs.is_dir() {
                std::fs::remove_dir_all(&abs)
                    .map_err(|e| AppError::Other(e.to_string()))?;
            } else {
                std::fs::remove_file(&abs)
                    .map_err(|e| AppError::Other(e.to_string()))?;
            }
        }
    } else {
        // Tracked file — restore from index.
        let mut checkout_opts = git2::build::CheckoutBuilder::new();
        checkout_opts.path(&path).force();
        r.checkout_index(None, Some(&mut checkout_opts))?;
    }
    Ok(())
}

#[tauri::command]
pub fn discard_all(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    // Safety net: snapshot the workdir before nuking it — the user can recover
    // via the Recovery tab if "Discard all" was the wrong button to click.
    crate::git::recovery::try_snapshot(
        r,
        crate::git::recovery::RecoveryKind::Discard,
        "discard all changes".to_string(),
    );

    // Collect untracked files/dirs before checkout so we can delete them.
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = r.statuses(Some(&mut status_opts))?;

    let workdir = r.workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
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
    r.checkout_index(None, Some(&mut checkout_opts))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Partial staging — apply a hand-crafted unified-diff patch to the index
// ---------------------------------------------------------------------------

/// Apply a unified diff patch to the repository index.
/// Used for line-level / hunk-level staging and unstaging:
/// the frontend builds the exact patch text and this command applies it.
#[tauri::command]
pub fn stage_patch(
    state: State<'_, AppState>,
    tab_id: String,
    patch: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    let diff = git2::Diff::from_buffer(patch.as_bytes())
        .map_err(|e| AppError::Other(format!("invalid patch: {e}")))?;

    r.apply(&diff, git2::ApplyLocation::Index, None)
        .map_err(|e| AppError::Other(format!("patch apply failed: {e}")))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn commit(
    state: State<'_, AppState>,
    tab_id: String,
    message: String,
    amend: bool,
) -> Result<String, AppError> {
    // ── Pre-commit veto ────────────────────────────────────────────────
    // Plugins subscribed to `on_pre_commit` may reject the commit by
    // returning a non-empty string from their handler. We stitch every
    // veto into the error message so the user sees which plugin
    // complained and why. Empty-reason vetoes are listed by plugin name
    // only.
    if let Ok(host) = state.lock_plugin_host() {
        let pre_ctx = serde_json::json!({
            "tab_id":  &tab_id,
            "message": &message,
            "amend":   amend,
        });
        let vetoes = host.collect_veto("on_pre_commit", &pre_ctx.to_string());
        if !vetoes.is_empty() {
            let mut lines: Vec<String> = Vec::with_capacity(vetoes.len());
            for (plugin, reason) in &vetoes {
                if reason.is_empty() {
                    lines.push(format!("{plugin}: blocked"));
                } else {
                    lines.push(format!("{plugin}: {reason}"));
                }
            }
            return Err(AppError::Other(format!(
                "Commit blocked by plugin(s):\n{}",
                lines.join("\n")
            )));
        }
    }

    // Scope the repos lock so it is released before firing plugin hooks
    // (Lua hooks may call git operations which would deadlock if the lock is held).
    let oid = {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    let sig = r.signature()?;
    let mut index = r.index()?;
    let tree_oid = index.write_tree()?;
    let tree = r.find_tree(tree_oid)?;

    if amend {
        // Use find_commit(revparse id) to avoid the peel_to_commit libgit2 bug.
        let head_oid = r.revparse_single("HEAD")
            .map_err(|_| AppError::Other("amend failed: no HEAD commit found".into()))?
            .id();
        let head_commit = r.find_commit(head_oid)?;
        let oid = head_commit.amend(
            Some("HEAD"),
            Some(&sig),
            Some(&sig),
            None,
            Some(&message),
            Some(&tree),
        )?;
        oid.to_string()
    } else {
        let parent_commits: Vec<git2::Commit<'_>> = match r.revparse_single("HEAD") {
            Ok(obj) => vec![r.find_commit(obj.id())?],
            Err(_) => vec![], // initial commit — no parent
        };
        let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
        let oid = r.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
        oid.to_string()
    }
    }; // repos lock released here

    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":  &tab_id,
            "oid":     &oid,
            "message": &message,
            "amend":   amend,
        });
        let _ = host.fire_hook("on_commit", &ctx.to_string());
    }
    Ok(oid)
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
fn read_conflicts(repo: &git2::Repository) -> Result<Vec<String>, AppError> {
    let mut index = repo.index().map_err(AppError::Git)?;
    // Force re-read so we see the state just written by cherrypick/revert.
    index.read(true).map_err(AppError::Git)?;
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

#[tauri::command]
pub fn cherry_pick(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
) -> Result<CherryPickResult, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let git_oid = git2::Oid::from_str(&oid)
        .map_err(|_| AppError::CommitNotFound(oid.clone()))?;
    let r = repo.inner();
    let commit = r.find_commit(git_oid)?;
    // Merge commits expose two diffs (one per parent); libgit2 needs
    // `mainline` to pick which side to apply. Default to parent 1 — the same
    // choice as `git cherry-pick -m 1` — so cherry-picking a merge replays
    // the work that was merged in.
    let mut opts = git2::CherrypickOptions::new();
    if commit.parent_count() > 1 {
        opts.mainline(1);
    }
    r.cherrypick(&commit, Some(&mut opts))?;
    let conflicted_files = read_conflicts(r)?;

    // On a clean apply libgit2 still leaves CHERRY_PICK_HEAD behind — that
    // blocks every subsequent pull / merge / rebase with "you have not
    // concluded your cherry-pick".  We do NOT auto-commit: the user wants
    // to inspect and commit the staged changes themselves (or amend / squash
    // / drop them) via the Stage area.  Just clear the sentinel so the repo
    // is no longer "stuck" in cherry-pick mode.
    let mut no_changes = false;
    if conflicted_files.is_empty() {
        no_changes = index_matches_head(r)?;
        r.cleanup_state()?;
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
fn index_matches_head(repo: &git2::Repository) -> Result<bool, AppError> {
    let mut index = repo.index()?;
    // Refresh so the comparison sees writes just produced by cherrypick().
    index.read(true)?;
    let index_tree_oid = index.write_tree()?;

    let head_obj    = repo.revparse_single("HEAD")?;
    let head_commit = repo.find_commit(head_obj.id())?;
    let head_tree   = head_commit.tree()?;

    Ok(index_tree_oid == head_tree.id())
}

#[tauri::command]
pub fn revert_commit(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
) -> Result<CherryPickResult, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let git_oid = git2::Oid::from_str(&oid)
        .map_err(|_| AppError::CommitNotFound(oid.clone()))?;
    let commit = repo.inner().find_commit(git_oid)?;
    // Merge commits have multiple parents, so libgit2 needs to know which
    // parent to treat as the mainline (the side to keep). Default to parent 1
    // — equivalent to `git revert -m 1` — which is what callers want in
    // virtually all cases (undo what was merged in, keep the receiving branch).
    let mut opts = git2::RevertOptions::new();
    if commit.parent_count() > 1 {
        opts.mainline(1);
    }
    let r = repo.inner();
    r.revert(&commit, Some(&mut opts))?;
    let conflicted_files = read_conflicts(r)?;

    // libgit2 leaves REVERT_HEAD behind even on a clean apply — same trap as
    // cherry-pick. Clear it so the repo isn't stuck in "revert mode" after
    // the user commits via the Stage area (git2's commit API doesn't know
    // about REVERT_HEAD; only the `git` CLI clears it implicitly).
    let mut no_changes = false;
    if conflicted_files.is_empty() {
        no_changes = index_matches_head(r)?;
        r.cleanup_state()?;
    }

    Ok(CherryPickResult {
        has_conflicts: !conflicted_files.is_empty(),
        conflicted_files,
        no_changes,
    })
}

/// Read the commit template from git's `commit.template` config entry (if set).
/// Returns `None` if no template is configured or the file cannot be read.
#[tauri::command]
pub fn get_git_commit_template(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;

    let config = match repo.inner().config() {
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
        let repo_path = repo.inner().workdir()
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
