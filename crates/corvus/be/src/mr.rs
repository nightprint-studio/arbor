//! `mr` domain — merge/pull-request network operations, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::mr`), but the context is [`CorvusState`] and the
//! provider comes from the reverse-channel registry ([`crate::provider`])
//! instead of the shell's `GitProviderRegistry`. Each handler resolves the
//! repo's `GitProvider` ([`provider_for_tab`](crate::provider::provider_for_tab),
//! a brief sync lock returning owned `Arc`s — no guard held across the `.await`),
//! then `.await`s the provider's REST call. The trait work is the shared
//! `corvus-git-provider-{api,github,gitlab}` crates, so results and
//! `ProviderError` wire strings are identical to in-process.
//!
//! **Hooks fire here** (plugin-relocation Wave 0): `on_mr_opened` (on create) and
//! `on_mr_updated` (on close/reopen/mark-ready) go to the co-located host with a
//! byte-identical payload, fired in the same position as in-process (after the
//! provider call's result is in hand, no provider handle held across the fire).
//! Auto-merge progress notifications still ride `state.emit("plugin:notification",
//! …)` — `CorvusState::emit` forwards to the same egress.
//!
//! What does **not** move here (the `SplitBroker` keeps routing these to the
//! shell in-process):
//!
//! - `merge_mr` — fires `on_mr_merged` but its GitHub branch-cleanup path
//!   (`delete_remote_via_push`) uses `state.lock_repos()` + the local git CLI
//!   (`git::remote::list_remotes`, `git::branch::delete_remote_branches`) to push
//!   `:refs/heads/<branch>`. `CorvusState` has no `RepoManager` and this is a
//!   local-git branch op, not a provider REST call — hard blocker.
//! - `mr_start_conflict_resolution` — the streaming-seam pilot: mints a job in the
//!   `state.jobs` registry (which `CorvusState` lacks), opens the repo via
//!   `state.lock_repos()`, and shells the local git merge-prep flow on a worker
//!   thread. Job-registry + local-git blocker.

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    AutoMergeOpts, CreateMrParams, MergeRequest, MergedMrHint, MrCapabilities, MrCommit, MrDetail,
    MrFeatureStatus, MrFile, MrFilter,
};

use crate::provider::{maybe_refresh, mr_id_from, pe, provider_for_tab};

// ---------------------------------------------------------------------------
// List MRs / PRs
// ---------------------------------------------------------------------------

/// List pull / merge requests for the active repo.
/// `state_filter`: "open" | "closed" | "merged" | "all"
#[arbor_rpc::handler]
async fn list_mrs(
    state:        &CorvusState,
    tab_id:       String,
    state_filter: Option<String>,
) -> Result<Vec<MergeRequest>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let filter = MrFilter {
        state: Some(state_filter.unwrap_or_else(|| "open".into())),
        ..Default::default()
    };
    resolved.provider.list_mrs(&resolved.repo, filter).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Get MR detail (with comments + checks)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn get_mr_detail(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<MrDetail, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.get_mr(&id).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Create MR / PR
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn create_mr(
    state:      &CorvusState,
    tab_id:     String,
    params:     CreateMrParams,
) -> Result<MergeRequest, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);

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

fn emit_auto_merge_ok(state: &CorvusState, number: u64) {
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge armed for #{number}"),
        "message": "Will merge automatically once required checks pass.",
        "level":   "success",
    }));
}

fn emit_auto_merge_err(state: &CorvusState, number: u64, err: &str) {
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
#[arbor_rpc::handler]
async fn get_mr_capabilities(
    state:  &CorvusState,
    tab_id: String,
) -> Result<MrCapabilities, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrCapabilities::default()),
    };
    maybe_refresh(&resolved.info.provider);

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
#[arbor_rpc::handler]
async fn probe_mr_feature(
    state:  &CorvusState,
    tab_id: String,
) -> Result<MrFeatureStatus, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrFeatureStatus::default()),
    };
    maybe_refresh(&resolved.info.provider);

    // Permissive on any probe/transport failure (the failing call will surface
    // a normal error later).
    Ok(resolved.provider.mr_feature_status(&resolved.repo).await.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Disable auto-merge
// ---------------------------------------------------------------------------

/// Cancel a previously-armed auto-merge / merge-when-pipeline-succeeds.
/// Idempotent: succeeds silently if auto-merge wasn't active.
#[arbor_rpc::handler]
async fn disable_mr_auto_merge(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.disable_auto_merge(&id).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Close / Reopen MR / PR
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn close_mr(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.close_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

#[arbor_rpc::handler]
async fn reopen_mr(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.reopen_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mark as ready for review (remove draft status)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn mark_mr_ready(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.mark_mr_ready(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Add comment
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn add_mr_comment(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
    body:   String,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.add_mr_comment(&id, &body).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File diffs / commits
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn get_mr_files(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrFile>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_files(&id).await.map_err(pe)
}

#[arbor_rpc::handler]
async fn get_mr_commits(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrCommit>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_commits(&id).await.map_err(pe)
}

#[arbor_rpc::handler]
async fn get_mr_commit_diff(
    state:  &CorvusState,
    tab_id: String,
    sha:    String,
) -> Result<Vec<MrFile>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved.provider.get_commit_diff(&resolved.repo, &sha).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Squash-merge hints for graph visualization
// ---------------------------------------------------------------------------

/// Returns a list of `{ sourceBranch, mergeCommitSha }` for all merged
/// PRs/MRs.  Used by the graph to draw accurate ghost edges for squash merges.
/// Returns an empty Vec (never errors) when the provider is not configured or
/// the token is missing — the graph simply shows no ghost edges in that case.
#[arbor_rpc::handler]
async fn get_merged_mr_hints(
    state:  &CorvusState,
    tab_id: String,
) -> Result<Vec<MergedMrHint>, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(vec![]),
    };
    maybe_refresh(&resolved.info.provider);

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
    state: &CorvusState,
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
    state.fire_hook(hook, ctx);
}

fn fire_mr_hook_by_number(
    state:    &CorvusState,
    hook:     &str,
    number:   u64,
    provider: &str,
) {
    let ctx = serde_json::json!({ "number": number, "provider": provider });
    state.fire_hook(hook, ctx);
}
