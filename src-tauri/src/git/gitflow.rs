//! `gitflow` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::gitflow`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (the gitflow IPC
//! handlers in `ipc/corvus/gitflow.rs`, plus the `GitFlowConfig` fields embedded
//! in app/repo config) are untouched. It injects the shell's resolved git
//! program (`GitCli`); Git Flow does not snapshot (finishes guard up front), so
//! no recovery binding is needed.
//!
//! When `corvus-be` serves gitflow out-of-process (next step), the backend will
//! call `corvus_git::gitflow` directly with its own `GitCli`.

use git2::Repository;

use corvus_git::prelude::GitCli;

use crate::error::Result;

// Re-export the data types so existing `crate::git::gitflow::*` paths resolve
// (the gitflow IPC handlers and the `GitFlowConfig` fields in app/repo config).
pub use corvus_git::prelude::{
    FlowFinishResult, FlowStartResult, GitFlowConfig, GitFlowStatus,
};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub fn get_gitflow_status(repo: &Repository, config: &GitFlowConfig) -> Result<GitFlowStatus> {
    Ok(corvus_git::gitflow::get_gitflow_status(repo, config)?)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

pub fn gitflow_init(repo: &Repository, config: &GitFlowConfig) -> Result<()> {
    Ok(corvus_git::gitflow::gitflow_init(&git(), repo, config)?)
}

pub fn gitflow_init_create_main(
    repo: &Repository,
    config: &GitFlowConfig,
    from_initial: bool,
) -> Result<()> {
    Ok(corvus_git::gitflow::gitflow_init_create_main(&git(), repo, config, from_initial)?)
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

pub fn feature_start(repo: &Repository, config: &GitFlowConfig, name: &str) -> Result<FlowStartResult> {
    Ok(corvus_git::gitflow::feature_start(&git(), repo, config, name)?)
}

pub fn feature_finish_or_pr(
    repo: &Repository,
    config: &GitFlowConfig,
    name: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    Ok(corvus_git::gitflow::feature_finish_or_pr(&git(), repo, config, name, force_pr)?)
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

pub fn release_start(repo: &Repository, config: &GitFlowConfig, version: &str) -> Result<FlowStartResult> {
    Ok(corvus_git::gitflow::release_start(&git(), repo, config, version)?)
}

pub fn release_finish_or_pr(
    repo: &Repository,
    config: &GitFlowConfig,
    version: &str,
    tag_message: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    Ok(corvus_git::gitflow::release_finish_or_pr(&git(), repo, config, version, tag_message, force_pr)?)
}

// ---------------------------------------------------------------------------
// Hotfix
// ---------------------------------------------------------------------------

pub fn hotfix_start(repo: &Repository, config: &GitFlowConfig, name: &str) -> Result<FlowStartResult> {
    Ok(corvus_git::gitflow::hotfix_start(&git(), repo, config, name)?)
}

pub fn hotfix_finish_or_pr(
    repo: &Repository,
    config: &GitFlowConfig,
    name: &str,
    tag_message: &str,
    force_pr: bool,
) -> Result<FlowFinishResult> {
    Ok(corvus_git::gitflow::hotfix_finish_or_pr(&git(), repo, config, name, tag_message, force_pr)?)
}
