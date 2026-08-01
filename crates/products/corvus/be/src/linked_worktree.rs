//! `linked_worktree` domain — worktree-link registry queries + mutations, served
//! **out-of-process** by corvus-be (full-move cutover, Phase 2).
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::linked_worktree`), ported onto the process-local
//! [`crate::worktree_links`] registry. Reads borrow the registry; writes go
//! through [`worktree_links::mutate`] (mutate + persist under the lock), then emit
//! `arbor://worktree-links-changed` so any open manager modal reloads. The two
//! member mutations fire `corvus:worktree_link_member_added` / `_removed` inline
//! after the registry lock is released (Lua hooks may call git ops; firing under
//! the guard would deadlock). Create / delete fire **no** hooks — only the
//! refresh event — matching the in-process copy.

use serde_json::json;

use corvus_core::prelude::{hooks, CorvusState};

use crate::worktree_links::{self, AliasEntry, AliasGroup, WorktreeLink};

/// Push the `arbor://worktree-links-changed` refresh event (empty payload).
fn emit_changed(state: &CorvusState) {
    state.emit("arbor://worktree-links-changed", json!({}));
}

// ── Read ──────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn list_worktree_links(state: &CorvusState) -> Result<Vec<WorktreeLink>, String> {
    Ok(worktree_links::registry(state).list())
}

#[arbor_rpc::handler]
fn get_worktree_link(state: &CorvusState, id: String) -> Result<Option<WorktreeLink>, String> {
    Ok(worktree_links::registry(state).get(&id).cloned())
}

#[arbor_rpc::handler]
fn get_worktree_link_for_repo(state: &CorvusState, repo_id: String) -> Result<Option<WorktreeLink>, String> {
    Ok(worktree_links::registry(state).find_by_repo(&repo_id).cloned())
}

// ── Write ─────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn create_worktree_link(
    state: &CorvusState,
    name: String,
    initial_repo_ids: Vec<String>,
) -> Result<WorktreeLink, String> {
    let link = worktree_links::mutate(state, |reg| reg.create(name, initial_repo_ids))?;
    emit_changed(state);
    Ok(link)
}

#[arbor_rpc::handler]
fn delete_worktree_link(state: &CorvusState, id: String) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.delete(&id))?;
    emit_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn rename_worktree_link(state: &CorvusState, id: String, name: String) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.rename(&id, name))?;
    emit_changed(state);
    Ok(())
}

/// Fires `corvus:worktree_link_member_added` ({link_id, repo_id}) after the lock drops.
#[arbor_rpc::handler]
fn add_worktree_link_member(state: &CorvusState, link_id: String, repo_id: String) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.add_member(&link_id, &repo_id))?;
    emit_changed(state);
    state.fire_hook(
        hooks::WORKTREE_LINK_MEMBER_ADDED,
        json!({ "link_id": link_id, "repo_id": repo_id }),
    );
    Ok(())
}

/// Fires `corvus:worktree_link_member_removed` ({link_id, repo_id}) after the lock drops.
#[arbor_rpc::handler]
fn remove_worktree_link_member(state: &CorvusState, link_id: String, repo_id: String) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.remove_member(&link_id, &repo_id))?;
    emit_changed(state);
    state.fire_hook(
        hooks::WORKTREE_LINK_MEMBER_REMOVED,
        json!({ "link_id": link_id, "repo_id": repo_id }),
    );
    Ok(())
}

#[arbor_rpc::handler]
fn set_worktree_link_sync_enabled(state: &CorvusState, link_id: String, enabled: bool) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.set_sync_enabled(&link_id, enabled))?;
    emit_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn set_worktree_link_member_sync_enabled(
    state: &CorvusState,
    link_id: String,
    repo_id: String,
    enabled: bool,
) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.set_member_sync_enabled(&link_id, &repo_id, enabled))?;
    emit_changed(state);
    Ok(())
}

// ── Aliases ───────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn add_alias_group(state: &CorvusState, link_id: String, members: Vec<AliasEntry>) -> Result<AliasGroup, String> {
    let group = worktree_links::mutate(state, |reg| reg.add_alias_group(&link_id, members))?;
    emit_changed(state);
    Ok(group)
}

#[arbor_rpc::handler]
fn update_alias_group(
    state: &CorvusState,
    link_id: String,
    group_id: String,
    members: Vec<AliasEntry>,
) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.update_alias_group(&link_id, &group_id, members))?;
    emit_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn remove_alias_group(state: &CorvusState, link_id: String, group_id: String) -> Result<(), String> {
    worktree_links::mutate(state, |reg| reg.remove_alias_group(&link_id, &group_id))?;
    emit_changed(state);
    Ok(())
}
