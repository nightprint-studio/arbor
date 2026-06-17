use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// The pure worktree git logic (enumeration, add/remove, project-type detection,
// and the `WorktreeInfo` / `ProjectType` types) moved into the Tauri-free
// `corvus-git` crate so the headless `corvus-be` shares it. Re-export the types
// here so existing `crate::git::worktree::{WorktreeInfo, ProjectType}` paths and
// the IDE machinery below keep resolving, and forward the three CLI functions
// through the shell's resolved git program (preserving the original
// no-`GitCli` signatures so existing callers compile unchanged).
pub use corvus_git::worktree::{ProjectType, WorktreeInfo};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// List all worktrees for the repository that owns the given path.
pub fn list_worktrees(repo_path: &Path, current_path: &Path) -> Result<Vec<WorktreeInfo>> {
    Ok(corvus_git::worktree::list_worktrees(&git(), repo_path, current_path)?)
}

/// Detect the primary project type by checking for well-known build files.
pub fn detect_project_type(path: &Path) -> ProjectType {
    corvus_git::worktree::detect_project_type(path)
}

/// Add a new linked worktree.
pub fn add_worktree(
    repo_path: &Path,
    dest_path: &str,
    branch: &str,
    new_branch: Option<&str>,
) -> Result<()> {
    Ok(corvus_git::worktree::add_worktree(&git(), repo_path, dest_path, branch, new_branch)?)
}

/// Remove a linked worktree.  Refuses if it is the main worktree.
pub fn remove_worktree(repo_path: &Path, worktree_path: &str) -> Result<()> {
    Ok(corvus_git::worktree::remove_worktree(&git(), repo_path, worktree_path)?)
}

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
