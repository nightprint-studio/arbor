//! `worktree` domain — pure git logic, Tauri-free.
//!
//! Worktree enumeration (`git worktree list --porcelain` + per-worktree
//! ahead/behind and dirty-file counts), worktree creation/removal, and
//! project-type detection. Lifted verbatim from the shell `crate::git::worktree`;
//! only the couplings the crate refuses are swapped — the git-program global
//! (`crate::git_cli::command()`) becomes an explicit [`GitCli`] threaded in by
//! the caller, and `AppError` becomes the crate's [`GitError`]. The serde shape
//! of [`WorktreeInfo`] / [`ProjectType`] is byte-identical to the shell's, so
//! the frontend wire payload is unchanged.
//!
//! NOT moved (stays shell-side): the IDE catalogue/detection/launch
//! (`BUILTIN_IDES`, `DetectedIde`, `detect_available_ides`, `open_in_ide`,
//! `spawn_ide_windows`) — those are process-spawn / detached-launch concerns,
//! not git, and the deferred `start_ide_detection` command still consumes them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Rust,
    NodeJs,
    JavaMaven,
    JavaGradle,
    Go,
    Python,
    DotNet,
    Cpp,
    Ruby,
    Php,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory.
    pub path: String,
    /// Checked-out branch name (None when detached HEAD).
    pub branch: Option<String>,
    /// Current HEAD commit SHA.
    pub head_sha: Option<String>,
    /// Short (7-char) HEAD commit SHA for display.
    pub head_short: Option<String>,
    /// The main worktree (where `.git/` lives). Cannot be removed.
    pub is_main: bool,
    /// Whether this worktree is currently locked (`git worktree lock`).
    pub is_locked: bool,
    /// True when this worktree path is the repo path open in the active tab.
    pub is_current: bool,
    /// Detected project/build-system type.
    pub project_type: ProjectType,
    /// Commits ahead of the remote upstream (0 when no upstream).
    pub ahead: usize,
    /// Commits behind the remote upstream (0 when no upstream).
    pub behind: usize,
    /// Number of locally modified/added/deleted files (0 when clean).
    pub changes_count: usize,
}

// ---------------------------------------------------------------------------
// Project-type detection
// ---------------------------------------------------------------------------

/// Detect the primary project type by checking for well-known build files.
pub fn detect_project_type(path: &Path) -> ProjectType {
    let markers: &[(&str, ProjectType)] = &[
        ("Cargo.toml",          ProjectType::Rust),
        ("pom.xml",             ProjectType::JavaMaven),
        ("build.gradle",        ProjectType::JavaGradle),
        ("build.gradle.kts",    ProjectType::JavaGradle),
        ("go.mod",              ProjectType::Go),
        ("package.json",        ProjectType::NodeJs),
        ("pyproject.toml",      ProjectType::Python),
        ("setup.py",            ProjectType::Python),
        ("requirements.txt",    ProjectType::Python),
        ("*.csproj",            ProjectType::DotNet),
        ("*.sln",               ProjectType::DotNet),
        ("CMakeLists.txt",      ProjectType::Cpp),
        ("Makefile",            ProjectType::Cpp),
        ("Gemfile",             ProjectType::Ruby),
        ("composer.json",       ProjectType::Php),
    ];

    for (pattern, project_type) in markers {
        if pattern.contains('*') {
            // Glob-style: list directory and match extension
            let ext = pattern.trim_start_matches("*.");
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some(ext) {
                        return project_type.clone();
                    }
                }
            }
        } else if path.join(pattern).exists() {
            return project_type.clone();
        }
    }
    ProjectType::Unknown
}

// ---------------------------------------------------------------------------
// List worktrees
// ---------------------------------------------------------------------------

/// List all worktrees for the repository that owns the given path.
/// Uses `git worktree list --porcelain` for reliable parsing.
pub fn list_worktrees(git: &GitCli, repo_path: &Path, current_path: &Path) -> Result<Vec<WorktreeInfo>> {
    let output = git
        .command()
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitError::Other(format!("git worktree list failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Other(format!("git worktree list: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_porcelain(git, &stdout, current_path)
}

/// Parse the `--porcelain` output of `git worktree list`.
///
/// Format (blocks separated by blank lines):
/// ```text
/// worktree /path/to/main
/// HEAD abc123def456...
/// branch refs/heads/main
///
/// worktree /path/to/linked
/// HEAD deadbeef...
/// branch refs/heads/feature
/// locked
/// ```
fn parse_worktree_porcelain(git: &GitCli, input: &str, current_path: &Path) -> Result<Vec<WorktreeInfo>> {
    let current_str = current_path.to_string_lossy().replace('\\', "/");

    let mut result = Vec::new();
    let mut is_first = true;

    for block in input.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut path_str: Option<String> = None;
        let mut head_sha: Option<String> = None;
        let mut branch: Option<String> = None;
        let mut is_locked = false;
        let mut is_bare = false;

        for line in block.lines() {
            if let Some(v) = line.strip_prefix("worktree ") {
                path_str = Some(v.replace('\\', "/"));
            } else if let Some(v) = line.strip_prefix("HEAD ") {
                head_sha = Some(v.to_owned());
            } else if let Some(v) = line.strip_prefix("branch ") {
                // strip "refs/heads/" prefix
                branch = Some(v.trim_start_matches("refs/heads/").to_owned());
            } else if line == "locked" || line.starts_with("locked ") {
                is_locked = true;
            } else if line == "bare" {
                is_bare = true;
            }
        }

        if is_bare {
            is_first = false;
            continue;
        }

        if let Some(path) = path_str {
            let is_main = is_first;
            is_first = false;

            let head_short = head_sha.as_deref().map(|s| s.chars().take(7).collect());
            let path_buf = PathBuf::from(&path);
            let project_type = detect_project_type(&path_buf);

            let norm_path = path.replace('\\', "/");
            let is_current = paths_equal(&norm_path, &current_str);

            let wt_path = PathBuf::from(&path);
            let (ahead, behind) = ahead_behind(git, &wt_path);
            let changes_count  = local_changes_count(git, &wt_path);

            result.push(WorktreeInfo {
                path,
                branch,
                head_sha,
                head_short,
                is_main,
                is_locked,
                is_current,
                project_type,
                ahead,
                behind,
                changes_count,
            });
        }
    }

    Ok(result)
}

/// Compare two forward-slash-normalised paths for "same worktree" identity.
/// Strips trailing slashes; on Windows, case-insensitive.
fn paths_equal(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches('/');
    let b = b.trim_end_matches('/');
    #[cfg(windows)]
    { a.eq_ignore_ascii_case(b) }
    #[cfg(not(windows))]
    { a == b }
}

// ---------------------------------------------------------------------------
// Per-worktree status helpers
// ---------------------------------------------------------------------------

/// Returns (ahead, behind) relative to the tracking remote.
/// Returns (0, 0) on any error (no upstream, detached HEAD, etc.).
fn ahead_behind(git: &GitCli, wt_path: &Path) -> (usize, usize) {
    let out = git
        .command()
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(wt_path)
        .output();

    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return (0, 0),
    };

    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.split_whitespace();
    let ahead  = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

/// Returns the number of changed files in the working tree (staged + unstaged).
/// Returns 0 on any error.
fn local_changes_count(git: &GitCli, wt_path: &Path) -> usize {
    let out = git
        .command()
        .args(["status", "--porcelain"])
        .current_dir(wt_path)
        .output();

    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count()
        }
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Add / Remove worktrees
// ---------------------------------------------------------------------------

/// Add a new linked worktree.
///
/// - If `new_branch` is Some, passes `-b <new_branch>` to create a new branch.
/// - Otherwise checks out the existing `branch` at `path`.
pub fn add_worktree(
    git: &GitCli,
    repo_path: &Path,
    dest_path: &str,
    branch: &str,
    new_branch: Option<&str>,
) -> Result<()> {
    let mut args = vec!["worktree", "add"];

    // Build args: add [--no-track] [-b new_branch] <path> [<branch>]
    if let Some(nb) = new_branch {
        args.push("-b");
        args.push(nb);
        args.push(dest_path);
        args.push(branch); // start point
    } else {
        args.push(dest_path);
        args.push(branch);
    }

    let output = git
        .command()
        .args(&args)
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitError::Other(format!("git worktree add failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Other(format!("git worktree add: {stderr}")));
    }
    Ok(())
}

/// Remove a linked worktree.  Refuses if it is the main worktree.
pub fn remove_worktree(git: &GitCli, repo_path: &Path, worktree_path: &str) -> Result<()> {
    // Safety check: never remove if worktree_path == repo_path
    let norm_repo = repo_path.to_string_lossy().replace('\\', "/");
    let norm_wt   = worktree_path.replace('\\', "/");

    if norm_wt == norm_repo || norm_repo.starts_with(&norm_wt) {
        return Err(GitError::Other(
            "Cannot remove the main worktree.".into(),
        ));
    }

    let output = git
        .command()
        .args(["worktree", "remove", "--force", worktree_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitError::Other(format!("git worktree remove failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Other(format!("git worktree remove: {stderr}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The porcelain parser is the only non-libgit2 / non-shell-out logic worth
    // pinning. `GitCli::new("git")` is harmless here because the fixtures
    // exercise blocks whose worktree paths do not exist, so the per-worktree
    // `git status` / `rev-list` shell-outs fail fast and yield (0, 0) / 0 —
    // exactly the behavior we assert on a non-repo path.

    #[test]
    fn parses_main_and_linked_blocks() {
        let input = "\
worktree /repo/main
HEAD abc123def456abc123def456abc123def456abcd
branch refs/heads/main

worktree /repo/feature
HEAD 0123456789abcdef0123456789abcdef01234567
branch refs/heads/feature
locked
";
        let git = GitCli::new("git");
        let out = parse_worktree_porcelain(&git, input, Path::new("/repo/main")).unwrap();
        assert_eq!(out.len(), 2);

        let main = &out[0];
        assert_eq!(main.path, "/repo/main");
        assert_eq!(main.branch.as_deref(), Some("main"));
        assert_eq!(main.head_short.as_deref(), Some("abc123d"));
        assert!(main.is_main);
        assert!(!main.is_locked);
        assert!(main.is_current);

        let feat = &out[1];
        assert_eq!(feat.path, "/repo/feature");
        assert_eq!(feat.branch.as_deref(), Some("feature"));
        assert!(!feat.is_main);
        assert!(feat.is_locked);
        assert!(!feat.is_current);
    }

    #[test]
    fn skips_bare_block_but_keeps_main_flag_on_next() {
        // A bare repo's first porcelain block is `bare`; it is skipped, and the
        // first *non-bare* worktree becomes the main one.
        let input = "\
worktree /repo/bare
bare

worktree /repo/wt
HEAD 0123456789abcdef0123456789abcdef01234567
branch refs/heads/main
";
        let git = GitCli::new("git");
        let out = parse_worktree_porcelain(&git, input, Path::new("/nowhere")).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "/repo/wt");
        assert!(out[0].is_main);
    }

    #[test]
    fn detached_head_has_no_branch() {
        let input = "\
worktree /repo/main
HEAD abc123def456abc123def456abc123def456abcd
detached
";
        let git = GitCli::new("git");
        let out = parse_worktree_porcelain(&git, input, Path::new("/repo/main")).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].branch.is_none());
    }
}
