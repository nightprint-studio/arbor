//! Shell-facing facade over the Tauri-free [`corvus_git_cli`] crate.
//!
//! Detection, the resolved-`git` `Command` builder, and the PortableGit download
//! moved into the crate (so the headless `corvus-be` shares them). This module
//! keeps the original `crate::git_cli::*` surface — same paths — but the fallible
//! calls map the crate's `GitCliError` back onto the shell's `AppError`, so the
//! ~20 in-process call sites are untouched.
//!
//! The keyring-coupled HTTP auth-arg helpers (`http_auth_args_for_url(s)`) are
//! NOT in the crate (they read stored credentials — shell-only state); they live
//! in [`crate::git::auth_args`] and are re-exported here so their call sites
//! (`crate::git_cli::http_auth_args_for_url`) are unchanged too.

use std::path::{Path, PathBuf};

use crate::error::Result;

// Infallible items re-exported verbatim (the crate owns the global detection
// state — in-process there is one instance, so behaviour is identical).
pub use corvus_git_cli::{
    clear_override, command, detect, download_supported, portable_dir, request_download_cancel,
    snapshot, DownloadProgress, GitCliState,
};
// Keyring-coupled auth-arg injection stays shell-side.
pub use crate::git::auth_args::{http_auth_args_for_url, http_auth_args_for_urls};

/// Verify a candidate git path (`--version`). Maps the crate error to `AppError`.
pub fn verify(path: &Path) -> Result<String> {
    Ok(corvus_git_cli::verify(path)?)
}

/// Set the resolved git path explicitly. Maps the crate error to `AppError`.
pub fn set_path(path: &Path, source: &'static str) -> Result<String> {
    Ok(corvus_git_cli::set_path(path, source)?)
}

/// Download + extract PortableGit (Windows). Maps the crate error to `AppError`.
pub async fn download_portable<F>(on_progress: F) -> Result<PathBuf>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    Ok(corvus_git_cli::download_portable(on_progress).await?)
}
