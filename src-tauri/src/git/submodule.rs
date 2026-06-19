//! `submodule` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::submodule`] (so the headless
//! `corvus-be` shares it). This module keeps the original shell-facing API —
//! same signatures, `AppError` results — so the in-process consumers (the
//! submodule panel + IPC handlers) are untouched. It injects the shell's
//! resolved git program (`GitCli`) and the **keyring-backed auth resolver**
//! (`crate::git_cli::http_auth_args_for_url`), which stays shell-side because
//! it reaches the OS credential store.
//!
//! When `corvus-be` serves submodule out-of-process, the backend will call
//! `corvus_git::submodule` directly with its own `GitCli` + auth binding.

use git2::Repository;

use corvus_git::prelude::GitCli;

use crate::error::Result;

// Re-export the data type so existing `crate::git::submodule::SubmoduleInfo`
// paths resolve.
pub use corvus_git::prelude::SubmoduleInfo;

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// Keyring-backed auth-arg resolver bound to the crate's injected seam.
/// Resolution touches the OS credential store, so it lives shell-side.
fn auth(url: &str) -> Vec<String> {
    crate::git_cli::http_auth_args_for_url(url)
}

pub fn list_submodules(repo: &Repository) -> Result<Vec<SubmoduleInfo>> {
    Ok(corvus_git::submodule::list_submodules(repo)?)
}

pub fn submodule_list_branches(repo: &Repository, sub_path: &str) -> Result<Vec<String>> {
    Ok(corvus_git::submodule::submodule_list_branches(repo, sub_path)?)
}

pub fn submodule_fetch(repo: &Repository, sub_path: &str) -> Result<()> {
    Ok(corvus_git::submodule::submodule_fetch(&git(), repo, sub_path, &auth)?)
}

pub fn submodule_pull(repo: &Repository, sub_path: &str) -> Result<String> {
    Ok(corvus_git::submodule::submodule_pull(&git(), repo, sub_path, &auth)?)
}

pub fn submodule_push(repo: &Repository, sub_path: &str) -> Result<String> {
    Ok(corvus_git::submodule::submodule_push(&git(), repo, sub_path, &auth)?)
}

pub fn submodule_checkout(repo: &Repository, sub_path: &str, branch: &str) -> Result<()> {
    Ok(corvus_git::submodule::submodule_checkout(&git(), repo, sub_path, branch)?)
}

pub fn update_submodules(repo_path: &str, recursive: bool) -> Result<()> {
    Ok(corvus_git::submodule::update_submodules(&git(), repo_path, recursive, &auth)?)
}

pub fn update_submodule(repo_path: &str, name: &str, recursive: bool) -> Result<()> {
    Ok(corvus_git::submodule::update_submodule(&git(), repo_path, name, recursive, &auth)?)
}
