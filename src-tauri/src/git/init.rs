//! `init` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::init`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (the repo-init
//! command flow, the `is_git_repo` / `get_git_identity` leaf reads, and the
//! `on_project_missing` validator) are untouched. It binds the **credential-
//! coupled push** step (which stays shell-side, in `crate::git::remote::push`)
//! to the crate's injected push closure.
//!
//! NOT moved (stays shell-side): provider repo creation — it lives in the
//! command layer (`ipc/corvus/repo.rs`) because it needs `AppState` to reach
//! the `GitProvider` registry. The command resolves the remote URL and passes
//! it through `InitRepoOptions::remote_url`, so the crate only sees a
//! fully-formed URL.

use git2::Repository;

use crate::error::Result;

// Re-export the data types so existing `crate::git::init::Init*` paths resolve.
pub use corvus_git::prelude::{InitOutcome, InitRepoOptions};

/// Returns true if `path` is inside a git repository (searches up the tree).
pub fn is_git_repo(path: &str) -> bool {
    corvus_git::init::is_git_repo(path)
}

/// Read user.name and user.email from the global git config.
pub fn get_git_identity() -> (String, String) {
    corvus_git::init::get_git_identity()
}

/// Initialise a new git repository at `path`.
/// Returns the init outcome (configured remote + push status).
pub async fn init(path: &str, options: &InitRepoOptions) -> Result<InitOutcome> {
    // Bind the credential-coupled push step: keyring resolution + smart-HTTP
    // auth live in `crate::git::remote::push`. The `e.to_string()` preserves the
    // exact message that flows into `InitOutcome.push_error` (byte-identical to
    // the pre-extraction behaviour).
    let push = |repo: &Repository, remote: &str, refspec: &str, force: bool| {
        crate::git::remote::push(repo, remote, refspec, force).map_err(|e| e.to_string())
    };
    Ok(corvus_git::init::init(path, options, &push).await?)
}
