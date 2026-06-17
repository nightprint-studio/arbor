//! `graph` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. The pure
//! git work is libgit2 (`git2`)-coupled and already lives in the reusable shell
//! module [`crate::git::graph`]; rather than extract a CLI-style `corvus-git`
//! module, these handlers delegate straight to it (no subprocess involved),
//! mirroring the `stash` domain's delegate-to-shell path.
//!
//! The generic `rpc` command already wraps dispatch in `spawn_blocking`, so the
//! per-handler `tokio::task::spawn_blocking` that the old async commands used is
//! dropped here; the brief-lock-then-reopen shape (copy the repo path under the
//! lock, release it, open a fresh `Repository` handle for the walk) is kept so
//! large repos don't starve other commands waiting for the lock. Behavior
//! (locks held, fresh-handle reopen, errors) is byte-identical.
//!
//! No hooks fire in this domain.
//!
//! Deferred (kept inline in `commands/graph_commands.rs`, handled by a later
//! emit/seam pass): `start_file_meta_scan` and `export_graph_svg` — both take
//! an `AppHandle` and emit progress events to the frontend.

use crate::error::AppError;
use crate::git::graph::{CommitDetail, GraphData, RepoFileEntry};
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn get_repo_files(state: &AppState, tab_id: String) -> Result<Vec<String>, AppError> {
    // Grab the repo path under a brief lock then do the walk on a fresh handle
    // so large repos don't freeze the IPC queue.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_repo_files(&repo)
}

#[corvus::handler]
fn get_files_last_commit(
    state: &AppState,
    tab_id: String,
    paths: Vec<String>,
) -> Result<Vec<RepoFileEntry>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_files_last_commit(&repo, paths)
}

/// Return a fast fingerprint of the repository's current ref state.
/// Used by the frontend cache to detect whether anything has changed
/// without loading the full graph.
///
/// Format: `<HEAD-SHA>|<ref1:sha1>,<ref2:sha2>,...` (refs sorted).
///
/// Only includes refs under `refs/heads/`, `refs/remotes/`, `refs/tags/` —
/// pseudo-refs like `FETCH_HEAD` and `ORIG_HEAD` are touched on every git
/// operation (even no-op fetches) and would make the fingerprint flap,
/// triggering pointless graph reloads from `refreshIfChanged`.
#[corvus::handler]
fn get_repo_fingerprint(state: &AppState, tab_id: String) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let inner = repo.inner();

    let head = inner
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default();

    let mut parts: Vec<String> = inner
        .references()
        .map_err(|e| AppError::Other(e.to_string()))?
        .flatten()
        .filter_map(|r| {
            let name = r.name()?.to_owned();
            // Skip pseudo-refs (FETCH_HEAD, ORIG_HEAD, MERGE_HEAD, …) — they
            // mutate on every operation regardless of actual state changes.
            if !(name.starts_with("refs/heads/")
                || name.starts_with("refs/remotes/")
                || name.starts_with("refs/tags/"))
            {
                return None;
            }
            let target = r.target()?.to_string();
            Some(format!("{}:{}", name, target))
        })
        .collect();
    parts.sort_unstable();

    Ok(format!("{}|{}", head, parts.join(",")))
}

#[corvus::handler]
fn get_graph(
    state: &AppState,
    tab_id: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    // Mutable handle so we can feed `stash_foreach` on the same repo
    // after the (immutable-only) graph walk finishes.
    let mut repo = git2::Repository::open(&repo_path)?;
    let mut data = crate::git::graph::load_graph(&repo, offset, limit)?;
    // Stash collection is cheap (few entries, no deep diff). Failures
    // are swallowed: a broken stash reflog shouldn't hide the graph.
    data.stashes = crate::git::stash::collect_stash_refs(&mut repo).unwrap_or_default();
    Ok(data)
}

#[corvus::handler]
fn get_graph_for_file(
    state: &AppState,
    tab_id: String,
    file_path: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::load_graph_for_file(&repo, &file_path, offset, limit)
}

/// Grabs the repo path and releases the mutex immediately, then opens a *fresh*
/// Repository handle so the scan does not starve other commands waiting for the
/// lock.
#[corvus::handler]
fn get_repo_file_tree(state: &AppState, tab_id: String) -> Result<Vec<RepoFileEntry>, AppError> {
    // Hold the mutex only long enough to copy the path string.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_repo_file_tree(&repo)
}

#[corvus::handler]
fn get_commit_detail(
    state: &AppState,
    tab_id: String,
    oid: String,
) -> Result<CommitDetail, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_commit_detail(&repo, &oid)
}
