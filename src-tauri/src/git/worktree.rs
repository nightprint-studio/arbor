use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Built-in IDE catalogue (shared between detection and launch)
// ---------------------------------------------------------------------------

pub struct BuiltinIde {
    pub id:   &'static str,
    pub name: &'static str,
    pub cmd:  &'static str,       // default command name (assumed in PATH)
    pub args: &'static [&'static str],
}

pub const BUILTIN_IDES: &[BuiltinIde] = &[
    BuiltinIde { id: "vscode",   name: "VS Code",       cmd: "code",     args: &["--new-window"] },
    BuiltinIde { id: "cursor",   name: "Cursor",         cmd: "cursor",   args: &["--new-window"] },
    BuiltinIde { id: "zed",      name: "Zed",            cmd: "zed",      args: &[] },
    BuiltinIde { id: "intellij", name: "IntelliJ IDEA",  cmd: "idea",     args: &[] },
    BuiltinIde { id: "webstorm", name: "WebStorm",        cmd: "webstorm", args: &[] },
    BuiltinIde { id: "pycharm",  name: "PyCharm",         cmd: "pycharm",  args: &[] },
    BuiltinIde { id: "rider",    name: "Rider",           cmd: "rider",    args: &[] },
    BuiltinIde { id: "clion",    name: "CLion",           cmd: "clion",    args: &[] },
    BuiltinIde { id: "goland",   name: "GoLand",          cmd: "goland",   args: &[] },
    BuiltinIde { id: "rubymine", name: "RubyMine",        cmd: "rubymine", args: &[] },
    BuiltinIde { id: "phpstorm", name: "PhpStorm",        cmd: "phpstorm", args: &[] },
    BuiltinIde { id: "sublime",  name: "Sublime Text",    cmd: "subl",     args: &[] },
    BuiltinIde { id: "rustrover", name: "RustRover",       cmd: "rustrover", args: &[] },
    BuiltinIde { id: "vim",      name: "Vim",              cmd: "vim",      args: &[] },
    BuiltinIde { id: "neovim",   name: "Neovim",           cmd: "nvim",     args: &[] },
];

/// Result of probing a single IDE on the current system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedIde {
    pub id:             String,
    pub name:           String,
    /// True if the executable was found (via PATH or path_override).
    pub available:      bool,
    /// Resolved executable path (None when not found).
    pub detected_path:  Option<String>,
}

/// Probe all built-in IDEs and return their availability.
/// `path_overrides` maps ide_id → custom executable path.
#[allow(dead_code)]
pub fn detect_available_ides(
    path_overrides: &std::collections::HashMap<String, String>,
) -> Vec<DetectedIde> {
    BUILTIN_IDES.iter().map(|ide| {
        // If the user supplied a custom path, use that first.
        if let Some(ov) = path_overrides.get(ide.id) {
            if !ov.is_empty() {
                let exists = Path::new(ov).exists() || which_command(ov).is_some();
                return DetectedIde {
                    id:            ide.id.to_string(),
                    name:          ide.name.to_string(),
                    available:     exists,
                    detected_path: if exists { Some(ov.clone()) } else { None },
                };
            }
        }
        // Otherwise probe the default command name.
        let found = which_command(ide.cmd);
        DetectedIde {
            id:            ide.id.to_string(),
            name:          ide.name.to_string(),
            available:     found.is_some(),
            detected_path: found,
        }
    }).collect()
}

/// Returns the resolved absolute path of `cmd` if it is found in PATH, else None.
#[allow(dead_code)]
fn which_command(cmd: &str) -> Option<String> {
    #[cfg(windows)]
    let output = std::process::Command::new("where").arg(cmd).no_window().output();
    #[cfg(not(windows))]
    let output = std::process::Command::new("which").arg(cmd).output();

    match output {
        Ok(o) if o.status.success() => {
            // `where` / `which` may return multiple lines; take the first.
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        }
        _ => None,
    }
}

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
pub fn list_worktrees(repo_path: &Path, current_path: &Path) -> Result<Vec<WorktreeInfo>> {
    let output = crate::git_cli::command()
        .args(["worktree", "list", "--porcelain"])
        .no_window()
        .current_dir(repo_path)
        .output()
        .map_err(|e| AppError::Other(format!("git worktree list failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!("git worktree list: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_worktree_porcelain(&stdout, current_path)
}

/// Parse the `--porcelain` output of `git worktree list`.
///
/// Format (blocks separated by blank lines):
/// ```
/// worktree /path/to/main
/// HEAD abc123def456...
/// branch refs/heads/main
///
/// worktree /path/to/linked
/// HEAD deadbeef...
/// branch refs/heads/feature
/// locked
/// ```
fn parse_worktree_porcelain(input: &str, current_path: &Path) -> Result<Vec<WorktreeInfo>> {
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
            let (ahead, behind) = ahead_behind(&wt_path);
            let changes_count  = local_changes_count(&wt_path);

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
fn ahead_behind(wt_path: &Path) -> (usize, usize) {
    let out = crate::git_cli::command()
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .no_window()
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
fn local_changes_count(wt_path: &Path) -> usize {
    let out = crate::git_cli::command()
        .args(["status", "--porcelain"])
        .no_window()
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

    let output = crate::git_cli::command()
        .args(&args)
        .no_window()
        .current_dir(repo_path)
        .output()
        .map_err(|e| AppError::Other(format!("git worktree add failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!("git worktree add: {stderr}")));
    }
    Ok(())
}

/// Remove a linked worktree.  Refuses if it is the main worktree.
pub fn remove_worktree(repo_path: &Path, worktree_path: &str) -> Result<()> {
    // Safety check: never remove if worktree_path == repo_path
    let norm_repo = repo_path.to_string_lossy().replace('\\', "/");
    let norm_wt   = worktree_path.replace('\\', "/");

    if norm_wt == norm_repo || norm_repo.starts_with(&norm_wt) {
        return Err(AppError::Other(
            "Cannot remove the main worktree.".into(),
        ));
    }

    let output = crate::git_cli::command()
        .args(["worktree", "remove", "--force", worktree_path])
        .no_window()
        .current_dir(repo_path)
        .output()
        .map_err(|e| AppError::Other(format!("git worktree remove failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Other(format!("git worktree remove: {stderr}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Open in IDE
// ---------------------------------------------------------------------------

/// Launch an IDE at the given path.
/// `ide_command` is the executable name or full path (e.g. "code", "idea", "cursor").
/// `extra_args` allows passing additional flags (e.g. ["--new-window"]).
///
/// The spawned process is detached so it keeps running when Arbor exits:
/// stdio handles are dropped, a new session/process group is used, and on
/// Windows the child attempts to break away from any enclosing job object
/// (with a graceful fallback when breakaway is not permitted).
pub fn open_in_ide(path: &str, ide_command: &str, extra_args: &[String]) -> Result<()> {
    #[cfg(windows)]
    {
        return spawn_ide_windows(path, ide_command, extra_args);
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(ide_command);
        cmd.args(extra_args);
        cmd.arg(path);
        cmd.process_group(0);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn()
            .map_err(|e| AppError::Other(format!("Failed to launch '{ide_command}': {e}")))?;
        Ok(())
    }
}

/// Windows-specific detached spawn with a two-stage fallback:
/// 1. `DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB` — best case, escapes a
///    parent Job Object with `KILL_ON_JOB_CLOSE` (happens in `cargo tauri dev`).
/// 2. If (1) fails with `ERROR_ACCESS_DENIED (5)` — the job doesn't allow
///    breakaway — retry with `DETACHED_PROCESS` alone. The IDE may still be
///    tied to the parent job in that case, but production Arbor (launched from
///    Explorer) isn't in such a job, so this branch is dev-mode only.
#[cfg(windows)]
fn spawn_ide_windows(path: &str, ide_command: &str, extra_args: &[String]) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32          = 0x0000_0008;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    // Many IDEs on Windows ship as .cmd/.bat shims, so go through `cmd /c`.
    let build = |flags: u32| {
        let mut c = std::process::Command::new("cmd");
        c.arg("/c").arg(ide_command);
        c.args(extra_args);
        c.arg(path);
        c.creation_flags(flags);
        c.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c
    };

    match build(DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(5) => {
            build(DETACHED_PROCESS)
                .spawn()
                .map(|_| ())
                .map_err(|e| AppError::Other(format!("Failed to launch '{ide_command}': {e}")))
        }
        Err(e) => Err(AppError::Other(format!("Failed to launch '{ide_command}': {e}"))),
    }
}
