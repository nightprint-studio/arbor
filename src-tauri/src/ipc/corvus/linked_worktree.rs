//! `linked_worktree` domain — registry queries and mutations routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. Every
//! handler takes `&AppState` (no `AppHandle`) and, when it mutates the registry,
//! pushes the `arbor://worktree-links-changed` refresh event through the backend
//! **event sink** ([`AppState::event_sink`]) instead of `app.emit`. That's the
//! same egress shape `corvus-be` will use once it splits out (sink → channel).
//! Behavior is byte-identical to the old inline commands — only the egress handle
//! changed.
//!
//! The two member mutations (`add_worktree_link_member` /
//! `remove_worktree_link_member`) used to fire `on_worktree_link_member_added` /
//! `_removed` inline. Those fire-and-forget hooks now live in `post_hooks.rs`
//! (the generic `rpc` path), reconstructed from the call params — the handlers
//! fire no hooks themselves.

use std::sync::Arc;

use arbor_ipc::prelude::EventSink;

use crate::error::AppError;
use crate::ipc::corvus;
use crate::linked_worktrees::{self, AliasEntry, AliasGroup, WorktreeLink};
use crate::AppState;

/// Push the `arbor://worktree-links-changed` refresh event through the backend
/// event sink so any open `WorktreeLinkManagerModal` reloads. Mirrors the old
/// `emit_changed(&app)` helper, byte-identical topic + (empty) payload.
fn emit_changed(sink: &Arc<dyn EventSink>) {
    sink.emit("arbor://worktree-links-changed", serde_json::json!({}));
}

// ── Read ──────────────────────────────────────────────────────────────────────

#[corvus::handler]
fn list_worktree_links(state: &AppState) -> Result<Vec<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.list())
}

#[corvus::handler]
fn get_worktree_link(state: &AppState, id: String) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.get(&id).cloned())
}

#[corvus::handler]
fn get_worktree_link_for_repo(
    state: &AppState,
    repo_id: String,
) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.find_by_repo(&repo_id).cloned())
}

// ── Write ─────────────────────────────────────────────────────────────────────

#[corvus::handler]
fn create_worktree_link(
    state: &AppState,
    name: String,
    initial_repo_ids: Vec<String>,
) -> Result<WorktreeLink, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let link = {
        let mut reg = state.lock_linked_worktrees()?;
        let l = reg.create(name, initial_repo_ids)?;
        linked_worktrees::save(&reg)?;
        l
    };
    emit_changed(&sink);
    Ok(link)
}

#[corvus::handler]
fn delete_worktree_link(state: &AppState, id: String) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.delete(&id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

#[corvus::handler]
fn rename_worktree_link(state: &AppState, id: String, name: String) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.rename(&id, name)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

/// Fires `on_worktree_link_member_added` ({link_id, repo_id}) — now from
/// `post_hooks.rs`, not inline.
#[corvus::handler]
fn add_worktree_link_member(
    state: &AppState,
    link_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.add_member(&link_id, &repo_id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

/// Fires `on_worktree_link_member_removed` ({link_id, repo_id}) — now from
/// `post_hooks.rs`, not inline.
#[corvus::handler]
fn remove_worktree_link_member(
    state: &AppState,
    link_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.remove_member(&link_id, &repo_id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

#[corvus::handler]
fn set_worktree_link_sync_enabled(
    state: &AppState,
    link_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.set_sync_enabled(&link_id, enabled)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

#[corvus::handler]
fn set_worktree_link_member_sync_enabled(
    state: &AppState,
    link_id: String,
    repo_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.set_member_sync_enabled(&link_id, &repo_id, enabled)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

// ── Aliases ───────────────────────────────────────────────────────────────────

#[corvus::handler]
fn add_alias_group(
    state: &AppState,
    link_id: String,
    members: Vec<AliasEntry>,
) -> Result<AliasGroup, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let group = {
        let mut reg = state.lock_linked_worktrees()?;
        let g = reg.add_alias_group(&link_id, members)?;
        linked_worktrees::save(&reg)?;
        g
    };
    emit_changed(&sink);
    Ok(group)
}

#[corvus::handler]
fn update_alias_group(
    state: &AppState,
    link_id: String,
    group_id: String,
    members: Vec<AliasEntry>,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.update_alias_group(&link_id, &group_id, members)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}

#[corvus::handler]
fn remove_alias_group(
    state: &AppState,
    link_id: String,
    group_id: String,
) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.remove_alias_group(&link_id, &group_id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&sink);
    Ok(())
}
