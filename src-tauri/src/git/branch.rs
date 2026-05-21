use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub full_ref: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub head_oid: String,
    pub head_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub target_oid: String,
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn list_local_branches(repo: &Repository) -> Result<Vec<BranchInfo>> {
    let mut out = Vec::new();

    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = branch?;
        let name = branch.name()?.unwrap_or("").to_string();
        let full_ref = branch.get().name().unwrap_or("").to_string();
        let is_head = branch.is_head();

        let head_commit = branch.get().peel_to_commit()?;
        let head_oid = head_commit.id().to_string();
        let head_summary = head_commit.summary().unwrap_or("").to_string();

        let (upstream, ahead, behind) = upstream_info(repo, &branch, &head_oid);

        out.push(BranchInfo {
            name,
            full_ref,
            is_head,
            upstream,
            ahead,
            behind,
            head_oid,
            head_summary,
        });
    }
    Ok(out)
}

pub fn list_remote_branches(repo: &Repository) -> Result<Vec<BranchInfo>> {
    let mut out = Vec::new();
    for branch in repo.branches(Some(BranchType::Remote))? {
        let (branch, _) = branch?;
        let name = branch.name()?.unwrap_or("").to_string();
        let full_ref = branch.get().name().unwrap_or("").to_string();
        let head_commit = branch.get().peel_to_commit()?;
        let head_oid = head_commit.id().to_string();
        let head_summary = head_commit.summary().unwrap_or("").to_string();
        out.push(BranchInfo {
            name,
            full_ref,
            is_head: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            head_oid,
            head_summary,
        });
    }
    Ok(out)
}

pub fn list_tags(repo: &Repository) -> Result<Vec<TagInfo>> {
    let mut out = Vec::new();
    repo.tag_foreach(|oid, name_bytes| {
        let name = String::from_utf8_lossy(name_bytes)
            .trim_start_matches("refs/tags/")
            .to_string();
        let target_oid = oid.to_string();
        // Try to peel to annotated tag message
        let message = repo.find_tag(oid).ok().and_then(|t| t.message().map(String::from));
        out.push(TagInfo { name, target_oid, message });
        true
    })?;
    Ok(out)
}

/// Returns the nearest ancestor tag reachable from HEAD (equivalent to
/// `git describe --tags --abbrev=0`). Returns `None` if there are no tags.
pub fn get_nearest_tag(repo: &Repository) -> Option<String> {
    let mut opts = git2::DescribeOptions::new();
    opts.describe_tags();
    repo.describe(&opts).ok().and_then(|d| {
        let mut fmt = git2::DescribeFormatOptions::new();
        fmt.abbreviated_size(0);
        d.format(Some(&fmt)).ok()
    })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub fn create_branch(repo: &Repository, name: &str, from_oid: &str) -> Result<BranchInfo> {
    let oid = git2::Oid::from_str(from_oid)
        .map_err(|_| AppError::CommitNotFound(from_oid.to_string()))?;
    let commit = repo.find_commit(oid)?;
    repo.branch(name, &commit, false)?;
    let branch = repo.find_branch(name, BranchType::Local)?;
    let head_commit = branch.get().peel_to_commit()?;
    Ok(BranchInfo {
        name: name.to_string(),
        full_ref: branch.get().name().unwrap_or("").to_string(),
        is_head: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        head_oid: head_commit.id().to_string(),
        head_summary: head_commit.summary().unwrap_or("").to_string(),
    })
}

pub fn delete_branch(repo: &Repository, name: &str) -> Result<()> {
    let mut branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|_| AppError::BranchNotFound(name.to_string()))?;
    branch.delete()?;
    Ok(())
}

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<BranchInfo> {
    let mut branch = repo
        .find_branch(old_name, BranchType::Local)
        .map_err(|_| AppError::BranchNotFound(old_name.to_string()))?;
    let renamed = branch.rename(new_name, false)?;
    let head_commit = renamed.get().peel_to_commit()?;
    Ok(BranchInfo {
        name: new_name.to_string(),
        full_ref: renamed.get().name().unwrap_or("").to_string(),
        is_head: renamed.is_head(),
        upstream: None,
        ahead: 0,
        behind: 0,
        head_oid: head_commit.id().to_string(),
        head_summary: head_commit.summary().unwrap_or("").to_string(),
    })
}

pub fn checkout_branch(repo: &Repository, name: &str) -> Result<()> {
    let branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|_| AppError::BranchNotFound(name.to_string()))?;
    let refname = branch
        .get()
        .name()
        .ok_or_else(|| AppError::Other("invalid ref name".into()))?
        .to_string();

    // Take a recovery snapshot whenever the workdir has changes — libgit2 on
    // Windows has been observed to silently skip files it cannot replace
    // (antivirus / file-locking), so a snapshot is the only reliable backstop.
    crate::git::recovery::try_snapshot(
        repo,
        crate::git::recovery::RecoveryKind::Checkout,
        format!("checkout branch '{name}'"),
    );

    let obj = repo.revparse_single(&refname)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refname)?;
    Ok(())
}

/// Checkout a remote-tracking branch by creating (if needed) a local branch of
/// the same short name pointing at the remote tip, setting it to track the
/// remote, then switching HEAD to it. `full_remote_name` is the form returned
/// by `Branch::name()` for `BranchType::Remote` (e.g. `"origin/patch/4.14"`).
/// Returns the resolved local branch short name (e.g. `"patch/4.14"`).
pub fn checkout_remote_as_local(repo: &Repository, full_remote_name: &str) -> Result<String> {
    let remote_branch = repo
        .find_branch(full_remote_name, BranchType::Remote)
        .map_err(|_| AppError::BranchNotFound(full_remote_name.to_string()))?;
    let commit = remote_branch.get().peel_to_commit()?;

    let local_name = strip_remote_prefix(repo, full_remote_name)?;

    if repo.find_branch(&local_name, BranchType::Local).is_err() {
        let mut local = repo.branch(&local_name, &commit, false)?;
        let _ = local.set_upstream(Some(full_remote_name));
    }

    crate::git::recovery::try_snapshot(
        repo,
        crate::git::recovery::RecoveryKind::Checkout,
        format!("checkout remote '{full_remote_name}' as local '{local_name}'"),
    );

    let refname = format!("refs/heads/{local_name}");
    let obj = repo.revparse_single(&refname)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refname)?;
    Ok(local_name)
}

/// Strip the leading remote name from a remote-tracking branch's short name.
/// Iterates configured remotes so that branches with slashes in their names
/// (e.g. `origin/patch/4.14`) resolve correctly.
fn strip_remote_prefix(repo: &Repository, full_remote_name: &str) -> Result<String> {
    let remotes = repo.remotes()?;
    for r in remotes.iter().flatten() {
        let prefix = format!("{r}/");
        if let Some(rest) = full_remote_name.strip_prefix(&prefix) {
            if !rest.is_empty() {
                return Ok(rest.to_string());
            }
        }
    }
    // Fallback: split on the first '/'.
    full_remote_name
        .split_once('/')
        .map(|(_, rest)| rest.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BranchNotFound(full_remote_name.to_string()))
}

pub fn checkout_commit_detached(repo: &Repository, oid_str: &str) -> Result<()> {
    let oid = git2::Oid::from_str(oid_str)
        .map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;

    let short = oid_str.get(..7).unwrap_or(oid_str);
    crate::git::recovery::try_snapshot(
        repo,
        crate::git::recovery::RecoveryKind::Checkout,
        format!("checkout commit {short} (detached)"),
    );

    let obj = commit.as_object();
    repo.checkout_tree(obj, None)?;
    repo.set_head_detached(oid)?;
    Ok(())
}

/// List local branches that are fully merged into `target` (a branch name or ref).
/// Skips the current HEAD branch and the target itself.
pub fn list_merged_branches(repo: &Repository, target: &str) -> Result<Vec<BranchInfo>> {
    // Resolve target to a commit OID.
    let target_oid = repo
        .revparse_single(target)
        .map_err(AppError::Git)?
        .peel_to_commit()
        .map_err(AppError::Git)?
        .id();

    // Canonical short name of target (for skipping).
    let target_short = target.trim_start_matches("refs/heads/");

    let mut out = Vec::new();
    for branch in repo.branches(Some(BranchType::Local)).map_err(AppError::Git)? {
        let (branch, _) = branch.map_err(AppError::Git)?;
        if branch.is_head() { continue; }
        let name = branch.name().map_err(AppError::Git)?.unwrap_or("").to_string();
        if name == target_short { continue; }

        let head_commit = branch.get().peel_to_commit().map_err(AppError::Git)?;
        let branch_oid = head_commit.id();

        // A branch is merged if it has 0 commits ahead of the target
        // (i.e. all its commits are already reachable from the target).
        let merged = branch_oid == target_oid
            || repo.graph_ahead_behind(branch_oid, target_oid)
                .map(|(ahead, _)| ahead == 0)
                .unwrap_or(false);
        if !merged { continue; }

        let head_oid = branch_oid.to_string();
        let (upstream, ahead, behind) = upstream_info(repo, &branch, &head_oid);
        out.push(BranchInfo {
            name,
            full_ref: branch.get().name().unwrap_or("").to_string(),
            is_head: false,
            upstream,
            ahead,
            behind,
            head_oid,
            head_summary: head_commit.summary().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// List remote tracking branches that are fully merged into `target` (a local branch name).
/// Skips `<remote>/HEAD` pseudo-refs and the remote counterpart of the target itself.
pub fn list_merged_remote_branches(repo: &Repository, target: &str) -> Result<Vec<BranchInfo>> {
    let target_oid = repo
        .revparse_single(target)
        .map_err(AppError::Git)?
        .peel_to_commit()
        .map_err(AppError::Git)?
        .id();

    // Short name used to skip the remote counterpart of the target (e.g. "main").
    let target_short = target.trim_start_matches("refs/heads/");

    let mut out = Vec::new();
    for branch in repo.branches(Some(BranchType::Remote)).map_err(AppError::Git)? {
        let (branch, _) = branch.map_err(AppError::Git)?;
        let name = branch.name().map_err(AppError::Git)?.unwrap_or("").to_string();

        // Skip <remote>/HEAD pseudo-refs
        if name.ends_with("/HEAD") { continue; }

        // Skip the remote counterpart of the target (e.g. if target="main", skip "origin/main")
        let short = name.splitn(2, '/').nth(1).unwrap_or(&name);
        if short == target_short { continue; }

        let head_commit = branch.get().peel_to_commit().map_err(AppError::Git)?;
        let branch_oid = head_commit.id();

        let merged = branch_oid == target_oid
            || repo.graph_ahead_behind(branch_oid, target_oid)
                .map(|(ahead, _)| ahead == 0)
                .unwrap_or(false);
        if !merged { continue; }

        out.push(BranchInfo {
            name,
            full_ref: branch.get().name().unwrap_or("").to_string(),
            is_head: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            head_oid: branch_oid.to_string(),
            head_summary: head_commit.summary().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// Delete remote branches via push. `names` are "remote/branch" format (e.g. "origin/feature-x").
/// Returns names of any that failed.
pub fn delete_remote_branches(repo: &Repository, names: &[String]) -> Vec<String> {
    let mut failed = Vec::new();
    for name in names {
        let parts: Vec<&str> = name.splitn(2, '/').collect();
        if parts.len() != 2 {
            failed.push(name.clone());
            continue;
        }
        let (remote_name, branch_name) = (parts[0], parts[1]);
        // Empty destination in refspec = delete on remote
        let refspec = format!(":refs/heads/{branch_name}");
        match crate::git::remote::push(repo, remote_name, &refspec, false) {
            Ok(_) => {
                // Remove local tracking ref so it disappears from the list immediately
                let tracking = format!("refs/remotes/{remote_name}/{branch_name}");
                if let Ok(mut r) = repo.find_reference(&tracking) {
                    let _ = r.delete();
                }
                // If <remote>/HEAD was a symbolic ref pointing to the just-deleted
                // branch, it would become dangling and break any subsequent
                // ref iteration / revwalk push_glob ("cannot be peeled").
                // Drop it — the next fetch will recreate it pointing to whatever
                // the remote's new default branch is.
                let head_ref = format!("refs/remotes/{remote_name}/HEAD");
                let points_to_deleted = repo
                    .find_reference(&head_ref)
                    .ok()
                    .and_then(|h| h.symbolic_target().map(|t| t.to_string()))
                    .map(|t| t == tracking)
                    .unwrap_or(false);
                if points_to_deleted {
                    if let Ok(mut h) = repo.find_reference(&head_ref) {
                        let _ = h.delete();
                    }
                }
            }
            Err(_) => failed.push(name.clone()),
        }
    }
    failed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRenameResult {
    /// The new remote-qualified name, e.g. "origin/main".
    pub new_full_name: String,
    /// True if a local branch with the old short name was found and renamed.
    pub local_renamed: bool,
    /// True if a local branch existed but the optional local-rename was skipped.
    pub local_skipped: bool,
}

/// Rename a remote branch by pushing the old tip to the new name and deleting
/// the old name. `full_remote_old_name` is the form returned by `Branch::name()`
/// for `BranchType::Remote` (e.g. `"origin/develop"`). When `rename_local` is
/// true AND a local branch with the same short name exists, the local branch
/// is renamed too and its upstream is re-pointed at the new remote ref.
pub fn rename_remote_branch(
    repo: &Repository,
    full_remote_old_name: &str,
    new_short_name: &str,
    rename_local: bool,
) -> Result<RemoteRenameResult> {
    // ── Resolve old remote tip ────────────────────────────────────────────
    let remote_branch = repo
        .find_branch(full_remote_old_name, BranchType::Remote)
        .map_err(|_| AppError::BranchNotFound(full_remote_old_name.to_string()))?;
    let old_oid = remote_branch
        .get()
        .target()
        .ok_or_else(|| AppError::Other(format!("'{full_remote_old_name}' has no target")))?;

    // ── Determine remote name + old short branch name ─────────────────────
    let (remote_name, old_short_name) = {
        let remotes = repo.remotes()?;
        let mut found: Option<(String, String)> = None;
        for r in remotes.iter().flatten() {
            let prefix = format!("{r}/");
            if let Some(rest) = full_remote_old_name.strip_prefix(&prefix) {
                if !rest.is_empty() {
                    found = Some((r.to_string(), rest.to_string()));
                    break;
                }
            }
        }
        found.ok_or_else(|| AppError::BranchNotFound(full_remote_old_name.to_string()))?
    };

    if new_short_name.trim().is_empty() {
        return Err(AppError::Other("new branch name cannot be empty".into()));
    }
    if new_short_name == old_short_name {
        return Err(AppError::Other("new name is the same as the old name".into()));
    }

    let old_remote_full_ref = format!("refs/remotes/{remote_name}/{old_short_name}");
    let new_tracking = format!("refs/remotes/{remote_name}/{new_short_name}");

    // Defensive: catch the modal slipping through with a name that already
    // exists locally as a tracking ref (stale local view of the server).
    if repo.find_reference(&new_tracking).is_ok() {
        return Err(AppError::Other(format!(
            "remote-tracking ref '{remote_name}/{new_short_name}' already exists locally — fetch and pick another name"
        )));
    }

    // ── Push using a temporary local branch as source ────────────────────
    // Pushing `refs/remotes/<r>/<old>:refs/heads/<new>` directly is unreliable:
    // libgit2 sometimes mis-negotiates the destination's "current" OID and
    // returns NotFastForward even when the destination doesn't exist. Going
    // through a local ref matches the canonical `git branch tmp <oid>; git
    // push origin tmp:<new>` flow that every git GUI uses.
    let temp_branch_name = format!(
        "arbor-rename-tmp-{}",
        uuid::Uuid::new_v4().as_simple()
    );
    let old_remote_tip = repo.find_commit(old_oid)?;
    let _ = repo.branch(&temp_branch_name, &old_remote_tip, false)?;

    let push_refspec = format!("refs/heads/{temp_branch_name}:refs/heads/{new_short_name}");
    let push_result = crate::git::remote::push(repo, &remote_name, &push_refspec, false);

    // Always clean up the temp local branch — succeeded or failed, it has no
    // value to the user.
    if let Ok(mut tmp) = repo.find_branch(&temp_branch_name, BranchType::Local) {
        let _ = tmp.delete();
    }

    if let Err(e) = push_result {
        let msg = format!("{e}");
        if msg.contains("non-fastforwardable") || msg.contains("NotFastForward") {
            return Err(AppError::Other(format!(
                "Cannot create '{remote_name}/{new_short_name}' on the server — a branch with that name already exists there with different commits. Fetch and choose a different name, or delete it manually first."
            )));
        }
        return Err(e);
    }

    // libgit2's push doesn't update local tracking refs when the source isn't
    // refs/heads/<new_name>, so write the new tracking ref ourselves.
    let _ = repo.reference(
        &new_tracking,
        old_oid,
        true,
        "rename: create new remote-tracking ref",
    );

    // ── Delete old remote ref ────────────────────────────────────────────
    let delete_refspec = format!(":refs/heads/{old_short_name}");
    crate::git::remote::push(repo, &remote_name, &delete_refspec, false)?;

    // Drop the old local tracking ref so it disappears from the list.
    if let Ok(mut r) = repo.find_reference(&old_remote_full_ref) {
        let _ = r.delete();
    }
    // Clean up <remote>/HEAD if it pointed to the now-deleted ref — otherwise
    // subsequent ref iteration / revwalk push_glob fails with "cannot be peeled".
    let head_ref = format!("refs/remotes/{remote_name}/HEAD");
    let points_to_deleted = repo
        .find_reference(&head_ref)
        .ok()
        .and_then(|h| h.symbolic_target().map(|t| t.to_string()))
        .map(|t| t == old_remote_full_ref)
        .unwrap_or(false);
    if points_to_deleted {
        if let Ok(mut h) = repo.find_reference(&head_ref) {
            let _ = h.delete();
        }
    }

    // ── Optionally rename matching local branch ──────────────────────────
    let local_exists = repo.find_branch(&old_short_name, BranchType::Local).is_ok();
    let mut local_renamed = false;
    if local_exists && rename_local {
        if let Ok(mut local) = repo.find_branch(&old_short_name, BranchType::Local) {
            if local.rename(new_short_name, false).is_ok() {
                local_renamed = true;
                if let Ok(mut renamed) = repo.find_branch(new_short_name, BranchType::Local) {
                    let upstream_short = format!("{remote_name}/{new_short_name}");
                    let _ = renamed.set_upstream(Some(&upstream_short));
                }
            }
        }
    }

    Ok(RemoteRenameResult {
        new_full_name: format!("{remote_name}/{new_short_name}"),
        local_renamed,
        local_skipped: local_exists && !rename_local,
    })
}

/// Delete multiple local branches. Returns names of any that failed.
pub fn delete_branches(repo: &Repository, names: &[String]) -> Vec<String> {
    let mut failed = Vec::new();
    for name in names {
        match repo.find_branch(name, BranchType::Local) {
            Ok(mut b) => { if b.delete().is_err() { failed.push(name.clone()); } }
            Err(_)    => { failed.push(name.clone()); }
        }
    }
    failed
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn upstream_info(repo: &Repository, branch: &git2::Branch, head_oid: &str) -> (Option<String>, usize, usize) {
    let upstream = branch.upstream().ok();
    if let Some(up) = upstream {
        let up_name = up.name().ok().flatten().map(String::from);
        let up_oid = up.get().target();
        if let (Some(local), Some(upstream_oid)) =
            (git2::Oid::from_str(head_oid).ok(), up_oid)
        {
            let (a, b) = repo.graph_ahead_behind(local, upstream_oid).unwrap_or((0, 0));
            return (up_name, a, b);
        }
        return (up_name, 0, 0);
    }
    (None, 0, 0)
}
