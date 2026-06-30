//! Linked-worktree sync orchestrator — cross-repo checkout propagation, run
//! **out-of-process** by corvus-be (full-move, Phase 3).
//!
//! Ported from the shell's `crate::linked_worktrees::orchestrator`. A branch
//! worktree-link handler (Phase 4) calls [`maybe_trigger_checkout_sync`] after a
//! successful checkout on the initiator tab; it snapshots the link, claims the
//! per-link recursion guard, resolves everything the worker needs while it still
//! holds `&CorvusState` (event sink + hook handle, the repo_id→path/name maps from
//! corvus-be's own repo registry, the open-tab set, the persistence path,
//! the git program + recovery policy) and moves it into a background thread. The
//! thread iterates the other members, checks each out stash-safe, persists
//! `last_sync_target`, and emits the aggregated `arbor://worktree-link-sync-*`
//! events + `on_worktree_link_sync_*` hooks — byte-identical topics/payloads.
//!
//! The recursion guard is process-local module state ([`SYNC_IN_PROGRESS`]) — the
//! OOP twin of the shell's `AppState::link_sync_in_progress`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::{CorvusState, HookDispatcher, PluginValue};
use corvus_git::prelude::{GitCli, SnapshotPolicy};
use serde_json::{json, Value};

use super::{aliases, MemberResult, MemberStatus, SyncSummary, SyncTarget, WorktreeLink};

/// Links currently syncing — the recursion guard (per-link). Process-local.
static SYNC_IN_PROGRESS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Everything the worker thread needs without reaching back for `&CorvusState`.
struct SyncCtx {
    sink: Arc<dyn EventSink>,
    hooks: Arc<HookDispatcher>,
    /// repo_id → working-directory path, for every registered repo.
    path_map: HashMap<String, PathBuf>,
    /// repo_id → display name, for self-sufficient progress events.
    name_map: HashMap<String, String>,
    /// (tab_id, path) for every open tab — targets `arbor://graph-refresh`.
    open_tabs: Vec<(String, String)>,
    /// The `linked_worktrees.toml` path to persist `last_sync_target` to.
    links_path: Option<String>,
    /// The git program for the per-member stash/checkout shell-outs.
    git_program: Option<String>,
    /// The recovery-snapshot policy for the pre-checkout snapshot.
    policy: SnapshotPolicy,
}

impl SyncCtx {
    fn emit(&self, event: &str, payload: Value) {
        self.sink.emit(event, payload);
    }
    fn fire_hook(&self, name: &str, ctx: &Value) {
        self.hooks.fire_blocking(name, PluginValue::from_json(ctx.clone()));
    }
}

/// repo_id → (path, display_name) maps, read from corvus-be's own canonical repo
/// registry (`repos.json`, reload-on-access) — no longer a shell-pushed snapshot.
fn repo_registry_maps(state: &CorvusState) -> (HashMap<String, PathBuf>, HashMap<String, String>) {
    let mut path_map = HashMap::new();
    let mut name_map = HashMap::new();
    for e in crate::workspace::registry::registry(state).list() {
        path_map.insert(e.id.clone(), PathBuf::from(&e.path));
        name_map.insert(e.id, e.display_name);
    }
    (path_map, name_map)
}

/// True if a link sync is currently running. (Reserved diagnostic — mirrors the
/// shell's `is_syncing`; no current caller.)
#[allow(dead_code)]
pub fn is_syncing(link_id: &str) -> bool {
    SYNC_IN_PROGRESS
        .lock()
        .map(|g| g.contains(link_id))
        .unwrap_or(false)
}

/// Trigger a link sync after a successful checkout on the initiator tab. Returns
/// immediately; the work runs in a background thread. Idempotent: no-ops if the
/// initiator repo is not in any link, the link/member has sync disabled, or a
/// sync is already in progress for that link (recursion guard).
pub fn maybe_trigger_checkout_sync(
    state: &CorvusState,
    initiator_tab_id: &str,
    initiator_repo_id: &str,
    branch: &str,
) {
    let tab_id = initiator_tab_id.to_string();
    let repo_id = initiator_repo_id.to_string();
    let branch = branch.to_string();

    // Snapshot the link + check opt-outs from the live registry.
    let link_snapshot: WorktreeLink = {
        let reg = super::registry(state);
        match reg.find_by_repo(&repo_id) {
            Some(l) if l.sync_enabled => {
                // Per-member opt-out: if the initiator member has sync_enabled
                // false, its checkouts don't propagate.
                if l.members.iter().any(|m| m.repo_id == repo_id && !m.sync_enabled) {
                    return;
                }
                l.clone()
            }
            _ => return,
        }
    };

    // Claim the recursion guard atomically.
    {
        let mut guard = SYNC_IN_PROGRESS.lock().unwrap_or_else(|p| p.into_inner());
        if guard.contains(&link_snapshot.id) {
            return;
        }
        guard.insert(link_snapshot.id.clone());
    }

    // Resolve everything the worker needs while `&CorvusState` is in hand.
    let (path_map, name_map) = repo_registry_maps(state);
    let ctx = SyncCtx {
        sink: state.event_sink(),
        hooks: state.hooks_handle(),
        path_map,
        name_map,
        open_tabs: state.open_tabs(),
        links_path: super::links_path(state),
        git_program: corvus_git_cli::snapshot().path.map(|p| p.to_string_lossy().into_owned()),
        policy: crate::repo::snapshot_policy(state),
    };

    std::thread::spawn(move || {
        run_orchestrator(ctx, tab_id, repo_id, branch, link_snapshot);
    });
}

fn run_orchestrator(
    ctx: SyncCtx,
    initiator_tab_id: String,
    initiator_repo_id: String,
    initiator_branch: String,
    link: WorktreeLink,
) {
    let link_id = link.id.clone();
    let link_name = link.name.clone();

    let start_payload = json!({
        "link_id": &link_id,
        "link_name": &link_name,
        "initiator_repo_id": &initiator_repo_id,
        "target_branch": &initiator_branch,
    });
    ctx.emit("arbor://worktree-link-sync-started", start_payload.clone());
    ctx.fire_hook("on_worktree_link_sync_started", &start_payload);

    let mut results: Vec<MemberResult> = Vec::new();
    let other_members: Vec<_> = link
        .members
        .iter()
        .filter(|m| m.repo_id != initiator_repo_id && m.sync_enabled)
        .collect();
    let total = other_members.len();

    for (idx, member) in other_members.iter().enumerate() {
        let target_branch =
            aliases::resolve_target_branch(&link, &initiator_repo_id, &initiator_branch, &member.repo_id);
        let repo_name = ctx.name_map.get(&member.repo_id).cloned();
        let emit_progress = |phase: &str, detail: Option<&str>| {
            ctx.emit(
                "arbor://worktree-link-sync-progress",
                json!({
                    "link_id":       &link_id,
                    "repo_id":       &member.repo_id,
                    "repo_name":     &repo_name,
                    "target_branch": &target_branch,
                    "index":         idx,
                    "total":         total,
                    "phase":         phase,
                    "detail":        detail,
                }),
            );
        };

        emit_progress("start", None);
        let path = match ctx.path_map.get(&member.repo_id) {
            Some(p) => p.clone(),
            None => {
                emit_progress("skipped", Some("not in registry"));
                results.push(MemberResult {
                    repo_id: member.repo_id.clone(),
                    status: MemberStatus::Skipped { reason: "repo not in registry".into() },
                });
                continue;
            }
        };
        if !path.exists() {
            emit_progress("skipped", Some("path missing on disk"));
            results.push(MemberResult {
                repo_id: member.repo_id.clone(),
                status: MemberStatus::Skipped { reason: "repo path missing on disk".into() },
            });
            continue;
        }

        let status = run_checkout_for_member(&path, &target_branch, &ctx.git_program, &ctx.policy);
        match &status {
            MemberStatus::Updated { branch } => emit_progress("ok", Some(branch)),
            MemberStatus::Skipped { reason } => emit_progress("skipped", Some(reason)),
            MemberStatus::SkippedMissing { branch } => {
                emit_progress("skipped", Some(&format!("branch '{branch}' not present locally")))
            }
            MemberStatus::Conflict { branch, files } => emit_progress(
                "conflict",
                Some(&format!("'{branch}' — {} conflicted file(s)", files.len())),
            ),
            MemberStatus::Error { message } => emit_progress("error", Some(message)),
        }
        results.push(MemberResult { repo_id: member.repo_id.clone(), status });
    }

    // Update last_sync_target + persist (locks the live registry).
    super::commit_sync_target(
        &link_id,
        SyncTarget {
            initiator_repo_id: initiator_repo_id.clone(),
            branch: initiator_branch.clone(),
            timestamp: super::now_secs(),
        },
        &ctx.links_path,
    );

    let summary = SyncSummary {
        link_id: link_id.clone(),
        link_name: link_name.clone(),
        target_branch: initiator_branch.clone(),
        initiator_repo_id: initiator_repo_id.clone(),
        results,
    };
    let summary_json = serde_json::to_value(&summary).unwrap_or(json!({}));
    ctx.emit("arbor://worktree-link-sync-done", summary_json.clone());
    ctx.emit("arbor://worktree-links-changed", json!({}));
    ctx.fire_hook("on_worktree_link_sync_done", &summary_json);

    // `arbor://graph-refresh` for each tab whose repo got a successful checkout.
    for r in summary
        .results
        .iter()
        .filter(|r| matches!(r.status, MemberStatus::Updated { .. }))
    {
        let Some(member_path) = ctx.path_map.get(&r.repo_id) else { continue };
        let new_branch = match &r.status {
            MemberStatus::Updated { branch } => branch.clone(),
            _ => String::new(),
        };
        for (tab_id, tab_path) in &ctx.open_tabs {
            if PathBuf::from(tab_path) == *member_path {
                ctx.emit(
                    "arbor://graph-refresh",
                    json!({ "tab_id": tab_id, "current_branch": &new_branch }),
                );
            }
        }
    }
    // Refresh the initiator's own tab too (its HEAD already moved before the
    // orchestrator ran, but the graph cache may still be stale).
    ctx.emit(
        "arbor://graph-refresh",
        json!({ "tab_id": &initiator_tab_id, "current_branch": &initiator_branch }),
    );

    // Release the recursion guard.
    if let Ok(mut guard) = SYNC_IN_PROGRESS.lock() {
        guard.remove(&link_id);
    }
}

/// Per-member checkout-with-stash. Mirrors the shell's `run_checkout_for_member`
/// using the shared `corvus-git` stash / recovery / branch ops bound to the
/// captured git program + recovery policy.
fn run_checkout_for_member(
    path: &Path,
    target_branch: &str,
    git_program: &Option<String>,
    policy: &SnapshotPolicy,
) -> MemberStatus {
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
        None => return MemberStatus::Error { message: "bare repo".into() },
    };

    let git = GitCli::from_optional(git_program.clone().map(PathBuf::from));

    let dirty = {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        match repo.statuses(Some(&mut opts)) {
            Ok(st) => st.iter().any(|s| s.status() != git2::Status::CURRENT),
            Err(e) => return MemberStatus::Error { message: format!("status failed: {e}") },
        }
    };

    let stashed = if dirty {
        match corvus_git::stash::stash_save(&git, &workdir, Some("link-sync pre-checkout"), true) {
            Ok(_) => true,
            Err(e) => return MemberStatus::Error { message: format!("stash failed: {e}") },
        }
    } else {
        false
    };

    // Recovery snapshot bound to the captured policy + git — fired both
    // explicitly (the shell's orchestrator does) and inside `checkout_branch`
    // (the shell's `snapshot_checkout` binding), matching the in-process pair.
    let snapshot = |repo: &Repository, summary: &str| {
        let _ = corvus_git::recovery::snapshot_with_policy(
            &git,
            repo,
            corvus_git::recovery::RecoveryKind::Checkout,
            summary,
            policy,
        );
    };
    snapshot(&repo, &format!("link-sync checkout '{target_branch}'"));

    if let Err(e) = corvus_git::branch::checkout_branch(&repo, target_branch, &snapshot) {
        if stashed {
            let _ = corvus_git::stash::stash_apply(&git, &mut repo, 0);
        }
        return MemberStatus::Error { message: format!("checkout failed: {e}") };
    }

    if stashed {
        match corvus_git::stash::stash_apply(&git, &mut repo, 0) {
            Ok(res) if res.has_conflicts => {
                return MemberStatus::Conflict {
                    branch: target_branch.to_string(),
                    files: res.conflicted_files,
                };
            }
            Ok(_) => {
                let _ = corvus_git::stash::stash_drop(&git, &mut repo, 0);
            }
            Err(e) => return MemberStatus::Error { message: format!("stash apply failed: {e}") },
        }
    }

    MemberStatus::Updated { branch: target_branch.to_string() }
}
