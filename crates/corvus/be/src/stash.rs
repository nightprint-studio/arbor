//! `stash` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (and function names → method names) as the shell's
//! in-process copy, but the context is [`CorvusState`] instead of the shell's
//! `AppState`: the repo is opened by the shell-pushed path
//! ([`CorvusState::repo_path`]) and the git program comes from
//! [`CorvusState::git_program`]. The git logic is the shared [`corvus_git`]
//! crate, so behavior — and error strings — are identical to in-process
//! (`GitError`'s `Display` is the same text the shell maps to `AppError`).
//!
//! **Hooks fire here, in-process to this backend.** `on_stash_push` /
//! `on_stash_pop` go through [`CorvusState::fire_hook`] to the plugin host
//! co-located in `corvus-be` (plugin-relocation Wave 0), after the repo handle
//! is dropped — same lock-then-fire discipline and payload as the shell's
//! in-process copy, so plugins see identical events whether stash runs in- or
//! out-of-process.
//!
//! **Recovery policy gap (known):** the force-apply / abort snapshots use
//! `SnapshotPolicy::default()` because this process has no app config yet. When
//! a user has customized the recovery size/extension limits, an OOP force-apply
//! or abort snapshots with the defaults. Closing this is the first concrete item
//! of the settings migration (push the configured policy to `CorvusState`, like
//! the git program).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{
    RecoveryKind, SnapshotPolicy, StashApplyResult, StashBlockingContent, StashEntry, StashRef,
};
use git2::Repository;

use crate::repo::{git, open};

#[arbor_rpc::handler]
fn list_stashes(state: &CorvusState, tab_id: String) -> Result<Vec<StashEntry>, String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::stash::list_stashes(&mut repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn list_graph_stash_refs(state: &CorvusState, tab_id: String) -> Result<Vec<StashRef>, String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::stash::collect_stash_refs(&mut repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn stash_save(
    state: &CorvusState,
    tab_id: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<StashEntry, String> {
    let workdir = {
        let repo = open(state, &tab_id)?;
        repo.workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf()
    };
    let entry = corvus_git::stash::stash_save(&git(state), &workdir, message.as_deref(), include_untracked)
        .map_err(|e| e.to_string())?;
    // Repo handle dropped above; fire inline so a Lua git op in the hook can't
    // deadlock. Payload mirrors the shell's in-process `stash_save`.
    state.fire_hook(
        "on_stash_push",
        serde_json::json!({
            "tab_id": tab_id,
            "index": entry.index,
            "message": entry.message,
            "include_untracked": include_untracked,
        }),
    );
    Ok(entry)
}

#[arbor_rpc::handler]
fn stash_apply(state: &CorvusState, tab_id: String, index: usize) -> Result<StashApplyResult, String> {
    let result = {
        let mut repo = open(state, &tab_id)?;
        corvus_git::stash::stash_apply(&git(state), &mut repo, index).map_err(|e| e.to_string())?
    };
    // Repo dropped; fire inline (drop:false, only when clean) — same as in-process.
    if !result.has_conflicts {
        state.fire_hook(
            "on_stash_pop",
            serde_json::json!({ "tab_id": tab_id, "index": index, "drop": false }),
        );
    }
    Ok(result)
}

#[arbor_rpc::handler]
fn stash_pop(state: &CorvusState, tab_id: String, index: usize) -> Result<StashApplyResult, String> {
    let result = {
        let mut repo = open(state, &tab_id)?;
        corvus_git::stash::stash_pop(&git(state), &mut repo, index).map_err(|e| e.to_string())?
    };
    // Repo dropped; fire inline (drop:true, only when clean) — same as in-process.
    if !result.has_conflicts {
        state.fire_hook(
            "on_stash_pop",
            serde_json::json!({ "tab_id": tab_id, "index": index, "drop": true }),
        );
    }
    Ok(result)
}

#[arbor_rpc::handler]
fn force_stash_apply(
    state: &CorvusState,
    tab_id: String,
    index: usize,
    files_to_delete: Vec<String>,
    files_to_keep: Vec<String>,
    drop_on_success: bool,
) -> Result<StashApplyResult, String> {
    let mut repo = open(state, &tab_id)?;
    let g = git(state);
    let snapshot = |r: &Repository, summary: &str| {
        let _ = corvus_git::recovery::snapshot_with_policy(
            &g,
            r,
            RecoveryKind::StashForceApply,
            summary,
            &SnapshotPolicy::default(),
        );
    };
    corvus_git::stash::force_stash_apply(
        &g,
        &snapshot,
        &mut repo,
        index,
        &files_to_delete,
        &files_to_keep,
        drop_on_success,
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn abort_stash_apply(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| "bare repository has no working directory".to_string())?
        .to_path_buf();
    let g = git(state);
    let snapshot = |r: &Repository, summary: &str| {
        let _ = corvus_git::recovery::snapshot_with_policy(
            &g,
            r,
            RecoveryKind::Other,
            summary,
            &SnapshotPolicy::default(),
        );
    };
    corvus_git::stash::abort_stash_apply_with_snapshot(&g, &workdir, Some(&repo), &snapshot)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn write_workdir_file(
    state: &CorvusState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| "bare repository has no working directory".to_string())?
        .to_path_buf();
    let abs = workdir.join(&path);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create parent dirs for {path}: {e}"))?;
    }
    let bytes = corvus_git::encoding::encode_for_disk(&content, encoding.as_deref());
    std::fs::write(&abs, &bytes).map_err(|e| format!("failed to write {path}: {e}"))?;
    Ok(())
}

#[arbor_rpc::handler]
fn get_stash_file_content(
    state: &CorvusState,
    tab_id: String,
    index: usize,
    path: String,
    encoding_override: Option<String>,
) -> Result<StashBlockingContent, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::stash::get_stash_file_content(&repo, index, &path, encoding_override.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn stash_drop(state: &CorvusState, tab_id: String, index: usize) -> Result<(), String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::stash::stash_drop(&git(state), &mut repo, index).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn stash_rename(
    state: &CorvusState,
    tab_id: String,
    index: usize,
    new_message: String,
) -> Result<StashEntry, String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::stash::stash_rename(&mut repo, index, &new_message).map_err(|e| e.to_string())
}
