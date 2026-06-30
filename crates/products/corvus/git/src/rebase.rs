//! `rebase` domain — pure git logic, Tauri-free.
//!
//! Lifted verbatim from the shell handler bodies and the old
//! `crate::git::rebase` module; only the couplings the crate refuses (the
//! git-program global, the `AppError` enum) are swapped for the crate's
//! explicit [`GitCli`] and [`GitError`]. The interactive-rebase flow keeps the
//! same temp-todo-file + `GIT_SEQUENCE_EDITOR`/`GIT_EDITOR` no-op shape, the
//! same "CONFLICT means paused, not failed" handling, and the same error
//! mapping (`GitError::Io` for spawn, `GitError::Other` for non-zero exit) so
//! behavior is byte-identical.
//!
//! The `on_rebase_start` / `on_rebase_abort` hooks fired around these calls
//! stay shell-side (they are fire-and-forget and need the broker).
//!
//! [`RebaseState::in_progress`] is derived by the caller from the live
//! `git2::RepositoryState`; this module only owns the wire type.

use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl std::fmt::Display for RebaseAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pick => write!(f, "pick"),
            Self::Reword => write!(f, "reword"),
            Self::Edit => write!(f, "edit"),
            Self::Squash => write!(f, "squash"),
            Self::Fixup => write!(f, "fixup"),
            Self::Drop => write!(f, "drop"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseTodoEntry {
    pub action: RebaseAction,
    pub short_oid: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseState {
    pub in_progress: bool,
    pub current_step: usize,
    pub total_steps: usize,
    pub conflicted_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Returns the list of commits between `base` and HEAD (exclusive, topological order).
pub fn get_rebase_todo(git: &GitCli, repo_path: &str, base: &str) -> Result<Vec<RebaseTodoEntry>> {
    let output = git
        .command()
        .args(["log", "--oneline", &format!("{base}..HEAD")])
        .current_dir(repo_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().rev() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            entries.push(RebaseTodoEntry {
                action: RebaseAction::Pick,
                short_oid: parts[0].to_string(),
                summary: parts[1].to_string(),
            });
        }
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Start an interactive rebase with a custom todo list.
///
/// We write the todo file ourselves and use `GIT_SEQUENCE_EDITOR=true` so git
/// accepts it without opening a real editor.
/// `GIT_EDITOR` is set to a no-op so that any `reword` step in the todo does
/// not block waiting for an interactive TTY editor — the existing commit message
/// is kept unchanged.
pub fn start_interactive_rebase(
    git: &GitCli,
    repo_path: &str,
    base: &str,
    todo: &[RebaseTodoEntry],
) -> Result<()> {
    // Write todo to a temp file with a unique name (process ID) to avoid
    // collisions when multiple repos are being rebased concurrently.
    let todo_content: String = todo
        .iter()
        .map(|e| format!("{} {} {}\n", e.action, e.short_oid, e.summary))
        .collect();

    let tmp_dir = std::env::temp_dir();
    let todo_path = tmp_dir.join(format!("arbor_rebase_todo_{}.tmp", std::process::id()));
    std::fs::write(&todo_path, &todo_content)?;

    let todo_path_str = todo_path.to_string_lossy();

    // Use a script/cmd that copies our file over whatever git passes as $1.
    #[cfg(windows)]
    let sequence_editor = format!("cmd /c copy /Y \"{}\" \"$1\"", todo_path_str);
    #[cfg(not(windows))]
    let sequence_editor = format!("cp '{}' \"$1\"", todo_path_str);

    let output = git
        .command()
        .args(["rebase", "-i", base])
        .env("GIT_SEQUENCE_EDITOR", &sequence_editor)
        // no-op editor: keeps the existing commit message for any 'reword' step
        // instead of hanging waiting for a TTY editor.
        .env("GIT_EDITOR", noop_editor())
        .current_dir(repo_path)
        .output()
        .map_err(GitError::Io)?;

    // Clean up the temp file regardless of outcome.
    let _ = std::fs::remove_file(&todo_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("CONFLICT") || stderr.contains("conflict") {
            // rebase paused on conflict — not an error per se
            return Ok(());
        }
        return Err(GitError::Other(stderr));
    }
    Ok(())
}

pub fn rebase_continue(git: &GitCli, repo_path: &str) -> Result<()> {
    // Set GIT_EDITOR to a no-op so that a 'reword' step in an interactive
    // rebase does not block waiting for an interactive TTY editor.
    // The existing commit message is kept unchanged.
    let output = git
        .command()
        .args(["rebase", "--continue"])
        .env("GIT_EDITOR", noop_editor())
        .current_dir(repo_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}

/// Returns a GIT_EDITOR value that exits 0 without modifying the file.
/// On all platforms that ship a `git` binary, `true` is available (Git for
/// Windows bundles it).  Running `true "$1"` ignores all arguments and exits 0,
/// so git keeps the existing commit message.
fn noop_editor() -> &'static str {
    "true"
}

pub fn rebase_abort(git: &GitCli, repo_path: &str) -> Result<()> {
    git_command(git, repo_path, &["rebase", "--abort"])
}

pub fn rebase_skip(git: &GitCli, repo_path: &str) -> Result<()> {
    git_command(git, repo_path, &["rebase", "--skip"])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn git_command(git: &GitCli, repo_path: &str, args: &[&str]) -> Result<()> {
    let output = git
        .command()
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(GitError::Io)?;

    if !output.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_action_display_mapping() {
        // The todo file is line-oriented `<action> <oid> <summary>`; the action
        // token must serialize to the exact git verbs or the rebase miswrites.
        assert_eq!(RebaseAction::Pick.to_string(), "pick");
        assert_eq!(RebaseAction::Reword.to_string(), "reword");
        assert_eq!(RebaseAction::Edit.to_string(), "edit");
        assert_eq!(RebaseAction::Squash.to_string(), "squash");
        assert_eq!(RebaseAction::Fixup.to_string(), "fixup");
        assert_eq!(RebaseAction::Drop.to_string(), "drop");
    }
}
