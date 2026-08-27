//! `submodule` domain — submodule listing + per-submodule / parent-level git
//! operations, served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::submodule`), but the context is [`CorvusState`]:
//! the repo is opened by the shell-pushed path and the git work is the shared
//! [`corvus_git::submodule`] crate, so behaviour + error strings are identical
//! (the crate's `GitError` `Display` mirrors `AppError`'s variant-for-variant,
//! so `e.to_string()` is the same wire string the shell produces).
//!
//! **No hooks fire for any submodule command.**
//!
//! Two couplings the corvus-git crate refuses get injected here:
//!   1. The git-binary invoker — [`crate::repo::git`].
//!   2. Auth-arg resolution — keyring-backed, so the keyring lookup stays
//!      shell-side. The reads (`list_submodules`, `submodule_list_branches`,
//!      `submodule_checkout`) need no auth; the network ops build an
//!      `AuthArgsResolver` (`url -> Vec<String>`) from the SAME reverse-channel
//!      credential seam the `remote` domain uses
//!      ([`crate::remote::auth_args_resolver`] → `__git_credentials`), which
//!      wraps the resolved `(user, pass)` into the host-scoped `-c …` argv.
//!      These run on the dispatch worker thread; the resolver blocks there and
//!      the serve loop's reader thread delivers the reply (the reverse-channel
//!      reentrancy) — no `spawn_blocking` (the in-process source spawned none).

use corvus_git::prelude::SubmoduleInfo;
use corvus_core::prelude::CorvusState;

use crate::remote::auth_args_resolver;
use crate::repo::{git, open, repo_path};

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn list_submodules(state: &CorvusState, tab_id: String) -> Result<Vec<SubmoduleInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::submodule::list_submodules(&repo).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Per-submodule operations
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn submodule_fetch(state: &CorvusState, tab_id: String, sub_path: String) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "submodule_fetch: no reverse channel for credentials".to_string())?;
    let auth = auth_args_resolver(host);
    corvus_git::submodule::submodule_fetch(&git(state), &repo, &sub_path, &auth)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn submodule_pull(state: &CorvusState, tab_id: String, sub_path: String) -> Result<String, String> {
    let repo = open(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "submodule_pull: no reverse channel for credentials".to_string())?;
    let auth = auth_args_resolver(host);
    corvus_git::submodule::submodule_pull(&git(state), &repo, &sub_path, &auth)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn submodule_push(state: &CorvusState, tab_id: String, sub_path: String) -> Result<String, String> {
    let repo = open(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "submodule_push: no reverse channel for credentials".to_string())?;
    let auth = auth_args_resolver(host);
    corvus_git::submodule::submodule_push(&git(state), &repo, &sub_path, &auth)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn submodule_checkout(
    state: &CorvusState,
    tab_id: String,
    sub_path: String,
    branch: String,
) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    corvus_git::submodule::submodule_checkout(&git(state), &repo, &sub_path, &branch)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn submodule_list_branches(
    state: &CorvusState,
    tab_id: String,
    sub_path: String,
) -> Result<Vec<String>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::submodule::submodule_list_branches(&repo, &sub_path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Parent-level update helpers (kept for backward compatibility)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn update_submodule(
    state: &CorvusState,
    tab_id: String,
    name: String,
    recursive: bool,
) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "update_submodule: no reverse channel for credentials".to_string())?;
    let auth = auth_args_resolver(host);
    corvus_git::submodule::update_submodule(&git(state), &path, &name, recursive, &auth)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn update_all_submodules(
    state: &CorvusState,
    tab_id: String,
    recursive: bool,
) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "update_all_submodules: no reverse channel for credentials".to_string())?;
    let auth = auth_args_resolver(host);
    corvus_git::submodule::update_submodules(&git(state), &path, recursive, &auth)
        .map_err(|e| e.to_string())
}
