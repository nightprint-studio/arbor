//! `merge` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::merge`), but the context is [`CorvusState`]: the
//! repo is opened by the shell-pushed path and the git program comes from
//! [`CorvusState::git_program`]. The pure git work is the shared [`corvus_git`]
//! crate, so behavior + error strings are identical to in-process — including
//! the `CONFLICTS:` prefix `merge_branch` puts on conflict failures so the FE
//! can redirect to the resolver.
//!
//! **No hooks fire in this domain** (the in-process copy fires none either), so
//! there is no lock-then-fire step. Conflict-resolution handlers
//! (`resolve_conflict`, `remove_conflict_file`, `resolve_stash_conflict`,
//! `complete_merge`) open the repo `mut`; subprocess-shelling handlers
//! (`merge_branch`, `abort_merge`) extract the workdir and drop the repo handle
//! before invoking the `git` CLI, mirroring the in-process plumbing.
//!
//! `merge_branch` takes **no** recovery snapshot (neither does in-process), so
//! the shared `SnapshotPolicy::default()` gap does not apply here.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{ConflictContent, ConflictPresence, MergeOutcome, MergeStrategy};

use crate::repo::{git, open};

#[arbor_rpc::handler]
fn get_conflict_content(
    state: &CorvusState,
    tab_id: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<ConflictContent, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::merge::get_conflict_content(&repo, &path, encoding_override.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_conflict_presence(
    state: &CorvusState,
    tab_id: String,
) -> Result<Vec<ConflictPresence>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::merge::get_conflict_presence(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn resolve_conflict(
    state: &CorvusState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::merge::resolve_conflict(&mut repo, &path, &content, encoding.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn remove_conflict_file(state: &CorvusState, tab_id: String, path: String) -> Result<(), String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::merge::remove_conflict_file(&mut repo, &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn resolve_stash_conflict(
    state: &CorvusState,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::merge::resolve_stash_conflict(&mut repo, &path, &content, encoding.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn complete_merge(state: &CorvusState, tab_id: String, message: String) -> Result<String, String> {
    let mut repo = open(state, &tab_id)?;
    corvus_git::merge::complete_merge(&mut repo, &message).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn abort_merge(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let workdir = {
        let repo = open(state, &tab_id)?;
        repo.workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf()
    }; // release the repo handle before spawning a subprocess
    corvus_git::merge::abort_merge(&git(state), &workdir).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn merge_branch(
    state: &CorvusState,
    tab_id: String,
    branch_name: String,
    strategy: Option<MergeStrategy>,
) -> Result<MergeOutcome, String> {
    let workdir = {
        let repo = open(state, &tab_id)?;
        repo.workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf()
    }; // lock released here
    corvus_git::merge::merge_branch(&git(state), &workdir, &branch_name, strategy.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_merge_message(state: &CorvusState, tab_id: String) -> Result<String, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::merge::get_merge_message(&repo).map_err(|e| e.to_string())
}
