//! Streaming blame Tauri command.
//!
//! The non-streaming diff/blame commands — and the deferred-emit
//! `get_workdir_diff_stream` — were migrated to broker handlers in
//! `ipc/corvus/diff.rs`. `get_file_blame_streaming` stays inline because it
//! takes a `tauri::ipc::Channel` and streams `BlameProgress` ticks to the
//! frontend — a later seam pass will move it.

use tauri::State;

use crate::error::AppError;
use crate::git::diff::BlameLine;
use crate::AppState;

/// Streaming blame: drives a determinate progress bar via `git blame
/// --incremental` while the history walk runs, returning the assembled lines
/// when it completes.  Total time is ≈ the same as the non-streaming blame —
/// the win is live feedback on large files instead of an indeterminate spinner.
///
/// Falls back to the libgit2 path (no progress ticks) when no `git` binary is
/// available, so the modal still works on a machine without git on PATH.
#[tauri::command]
pub async fn get_file_blame_streaming(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
    on_event: tauri::ipc::Channel<crate::git::blame_incremental::BlameProgress>,
) -> Result<Vec<BlameLine>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };

    let has_git = crate::git_cli::snapshot().path.is_some();

    tokio::task::spawn_blocking(move || {
        if has_git {
            crate::git::blame_incremental::run_incremental_blame(std::path::Path::new(&repo_path), &path, |p| {
                let _ = on_event.send(p);
            })
        } else {
            let repo = git2::Repository::open(&repo_path)?;
            crate::git::diff::get_file_blame(&repo, &path)
        }
    })
    .await
    .map_err(|e| AppError::Other(format!("get_file_blame_streaming task panicked: {e}")))?
}
