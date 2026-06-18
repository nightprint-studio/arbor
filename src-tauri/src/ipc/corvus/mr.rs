//! `mr` domain — merge/pull-request network operations routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it (async → `Kind::Async`, awaited on
//! the runtime). The handlers resolve the repo's `GitProvider` from `&AppState`
//! (`provider_for_tab`, a brief sync lock that returns owned `Arc`s — no guard
//! held across the `.await`), then `.await` the provider's REST call.
//!
//! Notifications that previously went through an `AppHandle` now use
//! `state.emit("plugin:notification", …)`; plugin hooks fire inline exactly as
//! before. `mr_start_conflict_resolution` (sync, Channel/streaming-coupled)
//! stays inline in `mr_commands` for a later seam pass.

use crate::ipc::corvus;
use crate::AppState;
use crate::error::{AppError, Result};
use crate::git_provider::mr_impl::{
    AutoMergeOpts, CreateMrParams, MergeRequest, MergedMrHint, MrCapabilities, MrDetail,
    MrFeatureStatus, MrFileDiff, MrCommit,
};
use crate::git_provider::{
    provider_for_tab, mr_id_from,
    types::{MrFilter, MergeOpts},
};

// ---------------------------------------------------------------------------
// ProviderError → AppError shim
// ---------------------------------------------------------------------------

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

// ---------------------------------------------------------------------------
// List MRs / PRs
// ---------------------------------------------------------------------------

/// List pull / merge requests for the active repo.
/// `state_filter`: "open" | "closed" | "merged" | "all"
#[corvus::handler]
async fn list_mrs(
    state:        &AppState,
    tab_id:       String,
    state_filter: Option<String>,
) -> Result<Vec<MergeRequest>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let filter = MrFilter {
        state: Some(state_filter.unwrap_or_else(|| "open".into())),
        ..Default::default()
    };
    resolved.provider.list_mrs(&resolved.repo, filter).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Get MR detail (with comments + checks)
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn get_mr_detail(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<MrDetail> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.get_mr(&id).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Create MR / PR
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn create_mr(
    state:      &AppState,
    tab_id:     String,
    params:     CreateMrParams,
) -> Result<MergeRequest> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // Capture the flags we echo back / use for auto-merge before `params` is
    // moved into `create_mr`.
    let auto          = params.auto_merge;
    let squash        = params.squash;
    let delete_branch = params.delete_branch;

    let mut mr = resolved.provider.create_mr(&resolved.repo, params).await.map_err(pe)?;
    // The provider may not echo these creation-time preferences back on the
    // returned MR — keep them so the detail modal's merge defaults are right.
    mr.squash        = squash;
    mr.delete_branch = delete_branch;

    if auto {
        // The provider resolves any extra handle it needs internally (GitHub's
        // GraphQL node id; GitLab's wait-for-mergeable poll).
        let id = mr_id_from(&resolved, mr.number);
        match resolved.provider.enable_auto_merge(&id, AutoMergeOpts { squash, delete_branch }).await {
            Ok(())  => emit_auto_merge_ok(state, mr.number),
            Err(e)  => emit_auto_merge_err(state, mr.number, &e.to_string()),
        }
    }

    fire_mr_hook(state, "on_mr_opened", &mr);
    Ok(mr)
}

fn emit_auto_merge_ok(state: &AppState, number: u64) {
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge armed for #{number}"),
        "message": "Will merge automatically once required checks pass.",
        "level":   "success",
    }));
}

fn emit_auto_merge_err(state: &AppState, number: u64, err: &str) {
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String = if short.len() > 240 { format!("{}…", &short[..240]) } else { short.to_string() };
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge could not be enabled on #{number}"),
        "message": trimmed,
        "level":   "error",
    }));
}

// ---------------------------------------------------------------------------
// Capability probe (drives "Enable auto-merge" enabled/disabled state)
// ---------------------------------------------------------------------------

/// Detect whether the active repo supports arming auto-merge / MWPS at
/// creation time.  Never fails — on any error it returns the permissive
/// default (`auto_merge_supported = true`) so the user can still try.
#[corvus::handler]
async fn get_mr_capabilities(
    state:  &AppState,
    tab_id: String,
) -> Result<MrCapabilities> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrCapabilities::default()),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // The provider returns the permissive default on any internal probe failure;
    // map a hard error (provider/transport) to the default too so the user can
    // still try.
    Ok(resolved.provider.auto_merge_allowed(&resolved.repo).await.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// MR/PR feature probe (drives sidebar EmptyState + Palette gating)
// ---------------------------------------------------------------------------

/// Probe whether the active repo accepts merge/pull requests at all.
/// Permissive on failure: any missing token or network error returns
/// `enabled = true` so the user can still try.  Only an explicit signal
/// from the provider (archived/disabled repo on GitHub, MR access level
/// "disabled" on GitLab) flips it to `false`.
#[corvus::handler]
async fn probe_mr_feature(
    state:  &AppState,
    tab_id: String,
) -> Result<MrFeatureStatus> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrFeatureStatus::default()),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // Permissive on any probe/transport failure (the failing call will surface
    // a normal error later).
    Ok(resolved.provider.mr_feature_status(&resolved.repo).await.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Disable auto-merge
// ---------------------------------------------------------------------------

/// Cancel a previously-armed auto-merge / merge-when-pipeline-succeeds.
/// Idempotent: succeeds silently if auto-merge wasn't active.
#[corvus::handler]
async fn disable_mr_auto_merge(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.disable_auto_merge(&id).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Merge MR / PR
// ---------------------------------------------------------------------------

/// `source_branch` is required when `delete_branch = true` (GitHub needs it for the branch name).
#[corvus::handler]
async fn merge_mr(
    state:         &AppState,
    tab_id:        String,
    number:        u64,
    merge_method:  Option<String>,
    squash:        Option<bool>,
    delete_branch: Option<bool>,
    source_branch: Option<String>,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let do_squash = squash.unwrap_or(false);
    let do_delete = delete_branch.unwrap_or(false);
    let strategy = if do_squash { Some("squash".into()) } else { merge_method };

    let id = mr_id_from(&resolved, number);
    let opts = MergeOpts {
        squash:         do_squash,
        delete_branch:  do_delete,
        commit_title:   None,
        commit_message: None,
        strategy,
    };
    resolved.provider.merge_mr(&id, opts).await.map_err(pe)?;

    // GitHub: delete remote branch via `git push :refs/heads/<branch>` because
    // the REST DELETE endpoint has been observed to silently no-op under some
    // OAuth/App configurations.  GitLab handles delete server-side via
    // `merge_gitlab_mr` (already passed `do_delete`).
    if resolved.info.provider == "github" && do_delete {
        match source_branch.as_deref() {
            Some(branch) => delete_remote_via_push(
                state, &tab_id, &resolved.info.remote_url,
                number, branch,
            ),
            None => emit_remote_delete_warning(
                state,
                number,
                "?",
                "no source branch was provided — cannot delete remote branch.",
            ),
        }
    }

    fire_mr_hook_by_number(state, "on_mr_merged", number, &resolved.info.provider);
    Ok(())
}

fn emit_remote_delete_warning(state: &AppState, number: u64, branch: &str, err: &str) {
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String = if short.len() > 400 { format!("{}…", &short[..400]) } else { short.to_string() };
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Remote branch '{branch}' not deleted after merging #{number}"),
        "message": trimmed,
        "level":   "warning",
    }));
}

/// Delete `<branch>` on the remote whose URL is `remote_url` using
/// `git push :refs/heads/<branch>`.  This is the same path the sidebar
/// "Delete remote branch" action uses; it relies on the repo's stored
/// git credentials rather than the provider's REST API token.
fn delete_remote_via_push(
    state:        &AppState,
    tab_id:       &str,
    remote_url:   &str,
    number:       u64,
    branch:       &str,
) {
    let push_outcome: Result<Vec<String>> = (|| {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(tab_id)?;
        let remote_name = crate::git::remote::list_remotes(repo.inner())?
            .into_iter()
            .find(|r| r.url == remote_url)
            .map(|r| r.name)
            .unwrap_or_else(|| "origin".into());
        let qualified = format!("{remote_name}/{branch}");
        Ok(crate::git::branch::delete_remote_branches(repo.inner(), &[qualified]))
    })();
    match push_outcome {
        Ok(failed) if failed.is_empty() => {} // success → silent; sidebar refresh shows the branch is gone
        Ok(_) => emit_remote_delete_warning(
            state, number, branch,
            "git push --delete failed (check the remote and your credentials)",
        ),
        Err(e) => emit_remote_delete_warning(
            state, number, branch, &e.to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Close / Reopen MR / PR
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn close_mr(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.close_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

#[corvus::handler]
async fn reopen_mr(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.reopen_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mark as ready for review (remove draft status) — not yet on the trait
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn mark_mr_ready(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.mark_mr_ready(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Add comment
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn add_mr_comment(
    state:  &AppState,
    tab_id: String,
    number: u64,
    body:   String,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.add_mr_comment(&id, &body).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File diffs / commits
// ---------------------------------------------------------------------------

#[corvus::handler]
async fn get_mr_files(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrFileDiff>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_files(&id).await.map_err(pe)
}

#[corvus::handler]
async fn get_mr_commits(
    state:  &AppState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrCommit>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_commits(&id).await.map_err(pe)
}

#[corvus::handler]
async fn get_mr_commit_diff(
    state:  &AppState,
    tab_id: String,
    sha:    String,
) -> Result<Vec<MrFileDiff>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider.get_commit_diff(&resolved.repo, &sha).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Squash-merge hints for graph visualization
// ---------------------------------------------------------------------------

/// Returns a list of `{ sourceBranch, mergeCommitSha }` for all merged
/// PRs/MRs.  Used by the graph to draw accurate ghost edges for squash merges.
/// Returns an empty Vec (never errors) when the provider is not configured or
/// the token is missing — the graph simply shows no ghost edges in that case.
#[corvus::handler]
async fn get_merged_mr_hints(
    state:  &AppState,
    tab_id: String,
) -> Result<Vec<MergedMrHint>> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(vec![]),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    if !resolved.provider.has_token() {
        return Ok(vec![]);
    }

    // GitHub: ask for "closed" (merged is a subset); GitLab: ask for "merged" directly.
    let state_filter = match resolved.info.provider.as_str() {
        "github" => "closed",
        _        => "merged",
    };
    let filter = MrFilter { state: Some(state_filter.into()), ..Default::default() };
    let mrs = match resolved.provider.list_mrs(&resolved.repo, filter).await {
        Ok(v)  => v,
        Err(_) => return Ok(vec![]),
    };

    let hints = mrs
        .into_iter()
        .filter_map(|mr| {
            let merge_sha = mr.merge_commit_sha?;
            Some(MergedMrHint {
                source_branch:    mr.source_branch,
                merge_commit_sha: merge_sha,
                head_sha:         mr.head_sha,
                base_sha:         mr.base_sha,
            })
        })
        .collect();

    Ok(hints)
}

// ---------------------------------------------------------------------------
// Hook firing helpers
// ---------------------------------------------------------------------------

fn fire_mr_hook(
    state: &AppState,
    hook:  &str,
    mr:    &MergeRequest,
) {
    let ctx = serde_json::json!({
        "number":        mr.number,
        "title":         mr.title,
        "source_branch": mr.source_branch,
        "target_branch": mr.target_branch,
        "provider":      mr.provider,
        "author":        mr.author.login,
        "web_url":       mr.web_url,
    });
    fire_hook(state, hook, ctx);
}

fn fire_mr_hook_by_number(
    state:    &AppState,
    hook:     &str,
    number:   u64,
    provider: &str,
) {
    let ctx = serde_json::json!({ "number": number, "provider": provider });
    fire_hook(state, hook, ctx);
}

fn fire_hook(state: &AppState, hook: &str, ctx: serde_json::Value) {
    state.fire_hook(hook, ctx);
}
