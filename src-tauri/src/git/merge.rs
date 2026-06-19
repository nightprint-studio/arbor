//! `merge` domain — shell-side re-exports of the `corvus-git` merge logic.
//!
//! All the git work — conflict three-way load, resolve/remove/stash-resolve,
//! the merge-commit finaliser, `git merge`, `git merge --abort`, the
//! `MERGE_MSG` reader, AND the streaming MR-prep flow
//! ([`prepare_mr_conflict_resolution`] & friends) — now lives in the Tauri-free
//! `corvus-git` crate and is reached directly by the broker handlers
//! (`ipc/corvus/merge.rs`, `ipc/corvus/mr.rs`).
//!
//! What stays here is only the thin shell binding for the MR-prep flow: it
//! injects the shell's resolved git program ([`crate::git_cli::snapshot`]) and
//! the keyring-backed auth-args resolver
//! ([`crate::git_cli::http_auth_args_for_url`]) — both of which the crate
//! refuses to hold — so the in-process `mr_start_conflict_resolution` handler
//! keeps its original 4-arg call site.

// Re-export the crate types so existing call sites (`use crate::git::merge::{…}`)
// keep compiling unchanged.
pub use corvus_git::merge::{MrPrepEvent, MrPrepOutcome};

use crate::error::Result;

/// Prepare the local workspace for resolving a pull/merge-request conflict.
///
/// Shell binding over [`corvus_git::merge::prepare_mr_conflict_resolution`]:
/// supplies the resolved git invoker and the keyring-backed auth-args resolver,
/// leaving the signature the in-process handler already uses
/// (`workdir, source, target, on_event`) intact.
pub fn prepare_mr_conflict_resolution(
    workdir:       &std::path::Path,
    source_branch: &str,
    target_branch: &str,
    on_event:      impl FnMut(MrPrepEvent<'_>),
) -> Result<MrPrepOutcome> {
    let git = corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path);
    Ok(corvus_git::merge::prepare_mr_conflict_resolution(
        &git,
        workdir,
        source_branch,
        target_branch,
        &auth,
        on_event,
    )?)
}

/// Keyring-backed auth-arg resolver bound to the crate's injected seam — same
/// shell-side binding the `submodule` wrapper uses. Resolution touches the OS
/// credential store, so it lives shell-side.
fn auth(url: &str) -> Vec<String> {
    crate::git_cli::http_auth_args_for_url(url)
}
