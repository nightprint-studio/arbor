//! Per-handler repo/git helpers shared by the domain modules.
//!
//! Every git domain served here resolves the same two things from `CorvusState`:
//! the repo for a `tab_id` (the shell pushes the path; there is no `RepoManager`)
//! and the git invoker (the program the shell pushed). Centralised so the error
//! string and the "open by pushed path" shape live in one place.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{GitCli, SnapshotPolicy};
use git2::Repository;

/// The git invoker for this backend. corvus-be self-detects its system `git`
/// (PATH / configured override / portable) into the `corvus-git-cli` process
/// global, so this reads that snapshot — `None` → `git` on `PATH`.
pub fn git(_state: &CorvusState) -> GitCli {
    GitCli::from_optional(corvus_git_cli::snapshot().path)
}

/// The recovery-snapshot policy from the corvus-owned global config (retention /
/// size / extension limits), or the built-in [`SnapshotPolicy::default`] when the
/// config has not been written yet. Centralised so every snapshotting domain
/// (stash, reset, recovery) reads the user-tuned policy from one place — closing
/// the OOP "always-default" gap (W0b). corvus-be owns `corvus/config.toml`, so
/// this reads its own `recovery` section ([`crate::corvus_config`]) rather than
/// the shell-pushed copy.
pub fn snapshot_policy(state: &CorvusState) -> SnapshotPolicy {
    crate::corvus_config::load(state).recovery
}

/// The diff context-line count to fall back on when the caller passes none: the
/// corvus-owned `diff.context_lines`, else libgit2's default of `3` (the
/// `DiffConfig::default`). Reads the corvus-owned global config rather than the
/// shell-pushed copy.
pub fn diff_context_lines(state: &CorvusState) -> u32 {
    crate::corvus_config::load(state).diff.context_lines
}

/// Whether workdir status scans detect renames/copies — the corvus-owned
/// `status.detect_renames`, else `true` (matches `StatusConfig::default`). The
/// dominant cost on repos with thousands of changed files, so the user can turn
/// it off; reads the corvus-owned global config rather than the shell-pushed copy.
pub fn status_detect_renames(state: &CorvusState) -> bool {
    crate::corvus_config::load(state).status.detect_renames
}

/// Resolve a tab to its repo path, or a clear error if the shell never
/// registered it (should not happen for an open tab). Used by domains that
/// shell out to the `git` CLI on a path (e.g. bisect) rather than open a handle.
pub fn repo_path(state: &CorvusState, tab_id: &str) -> Result<String, String> {
    state
        .repo_path(tab_id)
        .ok_or_else(|| format!("repo not registered for tab '{tab_id}'"))
}

/// Open the repo registered for `tab_id` as a libgit2 handle, or the same clear
/// error as [`repo_path`].
pub fn open(state: &CorvusState, tab_id: &str) -> Result<Repository, String> {
    Repository::open(repo_path(state, tab_id)?).map_err(|e| e.to_string())
}
