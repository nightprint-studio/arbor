// ---------------------------------------------------------------------------
// Linked-worktree sync orchestrator.
//
// Runs in a background thread spawned from the checkout command after the
// initiator's own checkout has succeeded.  Iterates over the other members
// of the link, performs the operation per member, and emits aggregated
// results.  Re-entry guarded via AppState.link_sync_in_progress.
//
// Model-D-shaped: the trigger takes `&AppState` (a corvus handler holds only
// that, never an `AppHandle`). It captures the pieces the worker thread needs —
// the event sink + hook dispatcher (both `Arc`, already `Send`), the two
// `Arc<Mutex<…>>` registries it writes after the work, and the repo path/name
// maps + open-tab snapshot resolved up-front — and moves them into the thread.
// No `AppHandle` crosses the boundary, so this code is unchanged when corvus
// splits out of the shell.
//
// V1: only LinkOperation::Checkout is supported.  Future ops plug in
// trivially by extending the match in `run_member_op`.
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;
use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
use serde_json::{json, Value};

use crate::linked_worktrees::{
    aliases, save, LinkOperation, MemberResult, MemberStatus, SyncSummary, SyncTarget,
    WorktreeLink, WorktreeLinkRegistry,
};
use crate::AppState;

// ---------------------------------------------------------------------------
// Captured runtime — everything the worker thread needs without an AppHandle.
// ---------------------------------------------------------------------------

/// The slice of `AppState` the orchestrator thread carries. Resolved in
/// [`maybe_trigger_checkout_sync`] (which has `&AppState`) and moved into the
/// spawned thread. `sink`/`hooks` are the event + hook egress; the two
/// registries are written after the per-member work; the maps + open-tab
/// snapshot are pre-resolved so the thread never reaches back for them.
struct SyncCtx {
    sink:                  Option<Arc<dyn EventSink>>,
    hooks:                 Arc<HookDispatcher>,
    linked_worktrees:      Arc<Mutex<WorktreeLinkRegistry>>,
    link_sync_in_progress: Arc<Mutex<HashSet<String>>>,
    /// repo_id → working-directory path, for every registered repo.
    path_map:              HashMap<String, PathBuf>,
    /// repo_id → display name, for self-sufficient progress events.
    name_map:              HashMap<String, String>,
    /// (tab_id, path) for every currently-open tab — used to target
    /// `arbor://graph-refresh` at the tabs showing a synced member.
    open_tabs:             Vec<(String, String)>,
}

impl SyncCtx {
    fn emit(&self, event: &str, payload: Value) {
        if let Some(s) = &self.sink {
            s.emit(event, payload);
        }
    }

    fn fire_hook(&self, name: &str, ctx: &Value) {
        self.hooks.fire_blocking(name, PluginValue::from_json(ctx.clone()));
    }
}

// ---------------------------------------------------------------------------
// Public entry point — spawns a thread.
// ---------------------------------------------------------------------------

/// Trigger a link sync after a successful checkout on the initiator tab.
///
/// Returns immediately; the actual work runs in a background thread that
/// emits Tauri events + Lua hooks.  Idempotent: silently no-ops if the
/// initiator's repo is not in any link, the link has sync disabled, or
/// a sync is already in progress for that link (recursion guard).
pub fn maybe_trigger_checkout_sync(
    state: &AppState,
    initiator_tab_id: &str,
    initiator_repo_id: &str,
    branch: &str,
) {
    tracing::info!(
        "linked-worktrees: checkout trigger — tab={} repo={} branch={}",
        initiator_tab_id, initiator_repo_id, branch
    );

    let tab_id  = initiator_tab_id.to_string();
    let repo_id = initiator_repo_id.to_string();
    let branch  = branch.to_string();

    // Snapshot the link + claim the recursion guard atomically.
    let link_snapshot: WorktreeLink = {
        let reg = match state.linked_worktrees.lock() {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("linked-worktrees: registry lock failed");
                return;
            }
        };
        match reg.find_by_repo(&repo_id) {
            Some(l) if l.sync_enabled => {
                // Per-member opt-out: if THIS repo (the initiator) has its
                // own sync_enabled=false, the checkout doesn't trigger any
                // propagation to siblings.
                let initiator_opted_out = l.members.iter()
                    .any(|m| m.repo_id == repo_id && !m.sync_enabled);
                if initiator_opted_out {
                    tracing::info!(
                        "linked-worktrees: matched link '{}' but initiator member opted out — skipping",
                        l.name
                    );
                    return;
                }
                tracing::info!(
                    "linked-worktrees: matched link '{}' (id={}, {} members)",
                    l.name, l.id, l.members.len()
                );
                l.clone()
            }
            Some(l) => {
                tracing::info!(
                    "linked-worktrees: matched link '{}' but sync_enabled=false — skipping",
                    l.name
                );
                return;
            }
            None => {
                tracing::info!(
                    "linked-worktrees: repo {} not a member of any link — skipping",
                    repo_id
                );
                return;
            }
        }
    };
    {
        let mut guard = match state.link_sync_in_progress.lock() {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("linked-worktrees: sync-in-progress lock failed");
                return;
            }
        };
        if guard.contains(&link_snapshot.id) {
            tracing::info!(
                "linked-worktrees: link {} already syncing — skipping recursive trigger",
                link_snapshot.id
            );
            return;
        }
        guard.insert(link_snapshot.id.clone());
    }

    // Resolve everything the worker needs while we still hold `&AppState`, then
    // hand it an AppHandle-free context.
    let ctx = SyncCtx {
        sink:                  state.event_sink(),
        hooks:                 state.hook_dispatcher.clone(),
        linked_worktrees:      Arc::clone(&state.linked_worktrees),
        link_sync_in_progress: Arc::clone(&state.link_sync_in_progress),
        path_map:              repo_path_map(state),
        name_map:              repo_name_map(state),
        open_tabs:             open_tab_snapshot(state),
    };

    tracing::info!(
        "linked-worktrees: spawning orchestrator for link '{}'",
        link_snapshot.name
    );
    std::thread::spawn(move || {
        run_orchestrator(
            ctx,
            tab_id,
            repo_id,
            branch,
            link_snapshot,
            LinkOperation::Checkout { branch: String::new() },
        );
    });
}

// ---------------------------------------------------------------------------
// Orchestrator — runs in the spawned thread.
// ---------------------------------------------------------------------------

fn run_orchestrator(
    ctx: SyncCtx,
    initiator_tab_id: String,
    initiator_repo_id: String,
    initiator_branch: String,
    link: WorktreeLink,
    _op: LinkOperation,
) {
    let link_id   = link.id.clone();
    let link_name = link.name.clone();

    ctx.emit("arbor://worktree-link-sync-started", json!({
        "link_id": &link_id,
        "link_name": &link_name,
        "initiator_repo_id": &initiator_repo_id,
        "target_branch": &initiator_branch,
    }));
    ctx.fire_hook("on_worktree_link_sync_started", &json!({
        "link_id": &link_id,
        "link_name": &link_name,
        "initiator_repo_id": &initiator_repo_id,
        "target_branch": &initiator_branch,
    }));

    let mut results: Vec<MemberResult> = Vec::new();
    let other_members: Vec<_> = link.members.iter()
        .filter(|m| m.repo_id != initiator_repo_id && m.sync_enabled)
        .collect();
    tracing::info!(
        "linked-worktrees: orchestrator running on {} non-initiator member(s)",
        other_members.len()
    );
    let total = other_members.len();
    for (idx, member) in other_members.iter().enumerate() {
        let target_branch = aliases::resolve_target_branch(
            &link, &initiator_repo_id, &initiator_branch, &member.repo_id,
        );
        let repo_name = ctx.name_map.get(&member.repo_id).cloned();
        let emit_progress = |phase: &str, detail: Option<&str>| {
            ctx.emit("arbor://worktree-link-sync-progress", json!({
                "link_id":       &link_id,
                "repo_id":       &member.repo_id,
                "repo_name":     &repo_name,
                "target_branch": &target_branch,
                "index":         idx,
                "total":         total,
                "phase":         phase,
                "detail":        detail,
            }));
        };

        emit_progress("start", None);
        let path = match ctx.path_map.get(&member.repo_id) {
            Some(p) => p.clone(),
            None => {
                tracing::warn!(
                    "linked-worktrees: member repo_id={} not found in registry, skipping",
                    member.repo_id
                );
                emit_progress("skipped", Some("not in registry"));
                results.push(MemberResult {
                    repo_id: member.repo_id.clone(),
                    status:  MemberStatus::Skipped { reason: "repo not in registry".into() },
                });
                continue;
            }
        };
        if !path.exists() {
            tracing::warn!(
                "linked-worktrees: member path {:?} missing on disk, skipping",
                path
            );
            emit_progress("skipped", Some("path missing on disk"));
            results.push(MemberResult {
                repo_id: member.repo_id.clone(),
                status:  MemberStatus::Skipped { reason: "repo path missing on disk".into() },
            });
            continue;
        }
        tracing::info!(
            "linked-worktrees: checking out '{}' on {:?}",
            target_branch, path
        );
        let status = run_checkout_for_member(&path, &target_branch);
        tracing::info!(
            "linked-worktrees: member result repo_id={} status={:?}",
            member.repo_id, status
        );
        // Translate the typed MemberStatus into the wire phase label.
        match &status {
            MemberStatus::Updated { branch }       => emit_progress("ok",      Some(branch)),
            MemberStatus::Skipped { reason }       => emit_progress("skipped", Some(reason)),
            MemberStatus::SkippedMissing { branch } => emit_progress(
                "skipped", Some(&format!("branch '{branch}' not present locally"))
            ),
            MemberStatus::Conflict { branch, files } => emit_progress(
                "conflict",
                Some(&format!("'{branch}' — {} conflicted file(s)", files.len())),
            ),
            MemberStatus::Error   { message }      => emit_progress("error",   Some(message)),
        }
        results.push(MemberResult {
            repo_id: member.repo_id.clone(),
            status,
        });
    }

    // Update last_sync_target + persist.
    let target = SyncTarget {
        initiator_repo_id: initiator_repo_id.clone(),
        branch:            initiator_branch.clone(),
        timestamp:         chrono::Utc::now().timestamp(),
    };
    if let Ok(mut reg) = ctx.linked_worktrees.lock() {
        let _ = reg.set_sync_target(&link_id, target);
        let _ = save(&reg);
    }

    let summary = SyncSummary {
        link_id:           link_id.clone(),
        link_name:         link_name.clone(),
        target_branch:     initiator_branch.clone(),
        initiator_repo_id: initiator_repo_id.clone(),
        results,
    };
    let summary_json = serde_json::to_value(&summary).unwrap_or(json!({}));
    ctx.emit("arbor://worktree-link-sync-done", summary_json.clone());
    ctx.emit("arbor://worktree-links-changed", json!({}));
    ctx.fire_hook("on_worktree_link_sync_done", &summary_json);

    // Emit `arbor://graph-refresh` for each tab whose repo got a successful
    // checkout — AppShell already wires that event to cacheStore.refresh, so
    // the second tab's branch/graph state updates without manual reload.
    for r in summary.results.iter().filter(|r| matches!(r.status, MemberStatus::Updated { .. })) {
        let Some(member_path) = ctx.path_map.get(&r.repo_id) else { continue; };
        let new_branch = match &r.status {
            MemberStatus::Updated { branch } => branch.clone(),
            _ => String::new(),
        };
        for (tab_id, tab_path) in &ctx.open_tabs {
            if PathBuf::from(tab_path) == *member_path {
                ctx.emit("arbor://graph-refresh", json!({
                    "tab_id":         tab_id,
                    "current_branch": &new_branch,
                }));
            }
        }
    }
    // Refresh the initiator's own tab too — the local checkout on the
    // initiator already updated HEAD before the orchestrator ran, but the
    // graph cache may still be stale when the user clicks fast.  Emit the
    // new branch so the TabBar's branch chip stays in sync as well.
    ctx.emit("arbor://graph-refresh", json!({
        "tab_id":         &initiator_tab_id,
        "current_branch": &initiator_branch,
    }));

    // Release the recursion guard.
    if let Ok(mut guard) = ctx.link_sync_in_progress.lock() {
        guard.remove(&link_id);
    }
}

// ---------------------------------------------------------------------------
// Per-member work — V1: checkout-with-stash.
// ---------------------------------------------------------------------------

fn run_checkout_for_member(path: &Path, target_branch: &str) -> MemberStatus {
    use git2::Repository;

    let mut repo = match Repository::open(path) {
        Ok(r) => r,
        Err(e) => return MemberStatus::Error { message: format!("open failed: {e}") },
    };

    if repo.find_branch(target_branch, git2::BranchType::Local).is_err() {
        return MemberStatus::SkippedMissing { branch: target_branch.to_string() };
    }

    if let Ok(head) = repo.head() {
        if head.shorthand() == Some(target_branch) {
            return MemberStatus::Skipped { reason: "already on target".into() };
        }
    }

    let workdir = match repo.workdir() {
        Some(w) => w.to_path_buf(),
        None    => return MemberStatus::Error { message: "bare repo".into() },
    };
    let dirty = {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        match repo.statuses(Some(&mut opts)) {
            Ok(st) => st.iter().any(|s| s.status() != git2::Status::CURRENT),
            Err(e) => return MemberStatus::Error { message: format!("status failed: {e}") },
        }
    };

    let stashed = if dirty {
        match crate::git::stash::stash_save(&workdir, Some("link-sync pre-checkout"), true) {
            Ok(_)  => true,
            Err(e) => return MemberStatus::Error { message: format!("stash failed: {e}") },
        }
    } else { false };

    crate::git::recovery::try_snapshot(
        &repo,
        crate::git::recovery::RecoveryKind::Checkout,
        format!("link-sync checkout '{target_branch}'"),
    );
    if let Err(e) = crate::git::branch::checkout_branch(&repo, target_branch) {
        if stashed {
            let _ = crate::git::stash::stash_apply(&mut repo, 0);
        }
        return MemberStatus::Error { message: format!("checkout failed: {e}") };
    }

    if stashed {
        match crate::git::stash::stash_apply(&mut repo, 0) {
            Ok(res) if res.has_conflicts => {
                return MemberStatus::Conflict {
                    branch: target_branch.to_string(),
                    files:  res.conflicted_files,
                };
            }
            Ok(_) => {
                let _ = crate::git::stash::stash_drop(&mut repo, 0);
            }
            Err(e) => {
                return MemberStatus::Error { message: format!("stash apply failed: {e}") };
            }
        }
    }

    MemberStatus::Updated { branch: target_branch.to_string() }
}

// ---------------------------------------------------------------------------
// Helpers — registry snapshots taken up-front (while `&AppState` is in hand).
// ---------------------------------------------------------------------------

fn repo_path_map(state: &AppState) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    if let Ok(reg) = state.repo_registry.lock() {
        for e in reg.list() {
            out.insert(e.id, PathBuf::from(e.path));
        }
    }
    out
}

/// Snapshot of repo_id → display_name for inclusion in
/// `arbor://worktree-link-sync-progress` events.  Resolving here saves a
/// round-trip and keeps the event payload self-sufficient when several syncs
/// run in quick succession.
fn repo_name_map(state: &AppState) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Ok(reg) = state.repo_registry.lock() {
        for e in reg.list() {
            out.insert(e.id, e.display_name);
        }
    }
    out
}

/// (tab_id, path) for every open tab, for targeting `arbor://graph-refresh`.
fn open_tab_snapshot(state: &AppState) -> Vec<(String, String)> {
    match state.repos.lock() {
        Ok(mgr) => mgr.list_open().into_iter().map(|(tab, path, _)| (tab, path)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Public helper — true if a link sync is currently running.
#[allow(dead_code)]
pub fn is_syncing(state: &AppState, link_id: &str) -> bool {
    match state.link_sync_in_progress.lock() {
        Ok(g) => g.contains(link_id),
        Err(_) => false,
    }
}
