//! `reset` / `tags` domain — pure git logic, Tauri-free.
//!
//! Lifted verbatim from the shell handler bodies; only the couplings the crate
//! refuses (the git-program global, the `AppError` enum) are swapped for the
//! crate's explicit [`GitCli`] and [`GitError`]. The shell keeps the OID
//! validation, the hard-reset recovery snapshot (config-loading), and the
//! `on_tag_*` hooks around these calls.

use git2::{ObjectType, Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

impl ResetMode {
    /// The `git reset` flag for this mode.
    fn flag(&self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

/// Shell out `git reset <flag> <oid>` in `workdir`.
///
/// Delegates to the `git` CLI instead of libgit2's `Repository::reset`. The
/// vendored libgit2 version bundled via `vendored-libgit2` has quirks that
/// caused `r.reset()` to behave like a checkout (move HEAD without moving the
/// current branch ref), which defeats the purpose of soft/mixed/hard resets.
///
/// The caller validates the oid + takes the recovery snapshot beforehand while
/// it still holds the repo lock.
pub fn run_reset(git: &GitCli, workdir: &std::path::Path, oid: &str, mode: ResetMode) -> Result<()> {
    let flag = mode.flag();

    tracing::info!("reset_to_commit: running `git reset {flag} {oid}` in {}", workdir.display());

    let out = git
        .command()
        .args(["reset", flag, oid])
        .current_dir(workdir)
        .output()
        .map_err(GitError::Io)?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    tracing::info!(
        "reset_to_commit: exit={:?} stdout={:?} stderr={:?}",
        out.status.code(), stdout.trim(), stderr.trim()
    );

    if !out.status.success() {
        let msg = if !stderr.trim().is_empty() { stderr.to_string() } else { stdout.to_string() };
        return Err(GitError::Other(format!("git reset failed: {}", msg.trim())));
    }

    Ok(())
}

/// Create a lightweight (`message = None`) or annotated tag at `oid`.
pub fn create_tag(repo: &Repository, name: &str, oid: &str, message: Option<&str>) -> Result<()> {
    let git_oid = Oid::from_str(oid).map_err(|_| GitError::CommitNotFound(oid.to_string()))?;
    let obj = repo.find_object(git_oid, Some(ObjectType::Commit))?;
    if let Some(msg) = message {
        let sig = repo.signature()?;
        repo.tag(name, &obj, &sig, msg, false)?;
    } else {
        repo.tag_lightweight(name, &obj, false)?;
    }
    Ok(())
}

/// Delete the tag `name`.
pub fn delete_tag(repo: &Repository, name: &str) -> Result<()> {
    repo.tag_delete(name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_mode_flag_mapping() {
        assert_eq!(ResetMode::Soft.flag(), "--soft");
        assert_eq!(ResetMode::Mixed.flag(), "--mixed");
        assert_eq!(ResetMode::Hard.flag(), "--hard");
    }
}
