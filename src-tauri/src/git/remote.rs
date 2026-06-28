//! `remote` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::remote`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (the init-repo
//! initial push, notes sync, linked-worktree sync) are untouched.
//!
//! It injects the two couplings the crate refuses:
//!   * the shell's resolved git program (`GitCli`) for `pull`'s CLI merge;
//!   * a **credential resolver** bound to `crate::auth::credential_store::
//!     resolve_credentials` (OS keyring). Keyring access stays entirely
//!     shell-side; the crate only ever receives the resolved `(user, pass)`.
//!
//! CONTRACT: `push` keeps the exact `(repo, remote_name, refspec, force)`
//! signature and `Result<()>` return — `crate::git::init` binds its injected
//! push closure to it.

use git2::Repository;

use corvus_git::prelude::GitCli;

use crate::error::Result;

// Re-export the data types so existing `crate::git::remote::{RemoteInfo,
// FetchResult}` paths resolve.
pub use corvus_git::prelude::{FetchResult, RemoteInfo};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// Credential resolver binding: keyring lookup lives shell-side, in
/// `crate::auth::credential_store`. Mapping its error to `String` preserves the
/// "log + treat as no credentials" behaviour the crate applies on `Err`.
fn resolve_credentials(url: &str) -> std::result::Result<Option<(String, String)>, String> {
    crate::auth::credential_store::resolve_credentials(url).map_err(|e| e.to_string())
}

pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>> {
    Ok(corvus_git::remote::list_remotes(repo)?)
}

pub fn fetch(repo: &Repository, remote_name: &str) -> Result<FetchResult> {
    Ok(corvus_git::remote::fetch(repo, remote_name, &resolve_credentials)?)
}

pub fn push(repo: &Repository, remote_name: &str, refspec: &str, force: bool) -> Result<()> {
    Ok(corvus_git::remote::push(repo, remote_name, refspec, force, &resolve_credentials)?)
}

pub fn pull(repo: &Repository, remote_name: &str) -> Result<()> {
    Ok(corvus_git::remote::pull(&git(), repo, remote_name, &resolve_credentials)?)
}
