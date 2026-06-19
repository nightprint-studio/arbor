//! `blame_incremental` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The streaming `git blame --incremental` parser moved into
//! [`corvus_git::diff`] (it lives next to the libgit2 blame path it mirrors).
//! This module keeps the original shell-facing API — `BlameProgress` and a
//! `run_incremental_blame(repo_path, path, on_progress)` that returns
//! `AppError` results — so the diff IPC handler (`ipc/corvus/diff.rs`) is
//! untouched. It injects the shell's resolved git program (`GitCli`).
//!
//! When `corvus-be` serves blame out-of-process, the backend will call
//! `corvus_git::diff::run_incremental_blame` directly with its own `GitCli`.

use std::path::Path;

use corvus_git::prelude::GitCli;

use crate::error::Result;
use crate::git::diff::BlameLine;

// Re-export so existing `crate::git::blame_incremental::BlameProgress` resolves.
pub use corvus_git::diff::BlameProgress;

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// Run `git blame --incremental` against HEAD and assemble `Vec<BlameLine>`,
/// invoking `on_progress` at every entry boundary (throttled to ~1% steps).
pub fn run_incremental_blame<F>(
    repo_path: &Path,
    path: &str,
    on_progress: F,
) -> Result<Vec<BlameLine>>
where
    F: FnMut(BlameProgress),
{
    Ok(corvus_git::diff::run_incremental_blame(&git(), repo_path, path, on_progress)?)
}
