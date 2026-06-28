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

/// The recovery-snapshot policy the shell pushed (retention / size / extension
/// limits), or the built-in [`SnapshotPolicy::default`] when no `"recovery"`
/// config section has been pushed. Centralised so every snapshotting domain
/// (stash, reset, recovery) reads the user-tuned policy from one place — closing
/// the OOP "always-default" gap (W0b). The wire shape is the shell's
/// `RecoveryConfig`, field-identical to `SnapshotPolicy`, so it deserializes
/// directly; a malformed/partial section also falls back to the default.
pub fn snapshot_policy(state: &CorvusState) -> SnapshotPolicy {
    state
        .config("recovery")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// The diff context-line count to fall back on when the caller passes none: the
/// shell-pushed `diff.context_lines`, else libgit2's default of `3`. Mirrors the
/// in-process `state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)`.
pub fn diff_context_lines(state: &CorvusState) -> u32 {
    state
        .config("diff")
        .and_then(|v| v.get("context_lines").and_then(|n| n.as_u64()))
        .map(|n| n as u32)
        .unwrap_or(3)
}

/// Whether workdir status scans detect renames/copies — the shell-pushed
/// `status.detect_renames`, else `true` (matches `StatusConfig::default`). The
/// dominant cost on repos with thousands of changed files, so the user can turn
/// it off; the OOP path reads the same toggle the in-process handler did.
pub fn status_detect_renames(state: &CorvusState) -> bool {
    state
        .config("status")
        .and_then(|v| v.get("detect_renames").and_then(|b| b.as_bool()))
        .unwrap_or(true)
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
