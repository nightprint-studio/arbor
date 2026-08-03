//! Built-in IDE catalogue + detached launch.
//!
//! Pure shell concern (process-spawn + IDE config), independent of git: split
//! out of `git/worktree.rs` once the worktree git operations migrated to
//! `corvus-git` / corvus-be. The IDE-detection streaming + config round-trips
//! live in the `ipc::corvus::ide` handlers; this module owns the catalogue and
//! the detached spawn.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Built-in IDE catalogue (shared between detection and launch)
// ---------------------------------------------------------------------------

pub struct BuiltinIde {
    pub id:   &'static str,
    pub name: &'static str,
    pub cmd:  &'static str,       // default command name (looked up per `locate`)
    pub args: &'static [&'static str],
    /// macOS application-bundle names to look for, most-preferred first, when no CLI launcher is
    /// found. On macOS an IDE *is* a `.app` bundle; the `cmd` shim above is an optional extra the
    /// user has to install by hand (VS Code's "Install 'code' command in PATH", JetBrains Toolbox's
    /// shell-scripts option), so a bundle is by far the more likely thing to exist. Empty for a
    /// terminal editor, which has no bundle.
    pub mac_apps: &'static [&'static str],
}

pub const BUILTIN_IDES: &[BuiltinIde] = &[
    BuiltinIde { id: "vscode",   name: "VS Code",       cmd: "code",     args: &["--new-window"],
                 mac_apps: &["Visual Studio Code.app"] },
    BuiltinIde { id: "cursor",   name: "Cursor",         cmd: "cursor",   args: &["--new-window"],
                 mac_apps: &["Cursor.app"] },
    BuiltinIde { id: "zed",      name: "Zed",            cmd: "zed",      args: &[],
                 mac_apps: &["Zed.app"] },
    // JetBrains bundles carry their edition in the name, so list every spelling: Toolbox installs
    // "IntelliJ IDEA.app" / "…Ultimate.app", a standalone download "…CE.app".
    BuiltinIde { id: "intellij", name: "IntelliJ IDEA",  cmd: "idea",     args: &[],
                 mac_apps: &["IntelliJ IDEA.app", "IntelliJ IDEA Ultimate.app", "IntelliJ IDEA CE.app"] },
    BuiltinIde { id: "webstorm", name: "WebStorm",        cmd: "webstorm", args: &[],
                 mac_apps: &["WebStorm.app"] },
    BuiltinIde { id: "pycharm",  name: "PyCharm",         cmd: "pycharm",  args: &[],
                 mac_apps: &["PyCharm.app", "PyCharm Professional.app", "PyCharm CE.app"] },
    BuiltinIde { id: "rider",    name: "Rider",           cmd: "rider",    args: &[],
                 mac_apps: &["Rider.app"] },
    BuiltinIde { id: "clion",    name: "CLion",           cmd: "clion",    args: &[],
                 mac_apps: &["CLion.app"] },
    BuiltinIde { id: "goland",   name: "GoLand",          cmd: "goland",   args: &[],
                 mac_apps: &["GoLand.app"] },
    BuiltinIde { id: "rubymine", name: "RubyMine",        cmd: "rubymine", args: &[],
                 mac_apps: &["RubyMine.app"] },
    BuiltinIde { id: "phpstorm", name: "PhpStorm",        cmd: "phpstorm", args: &[],
                 mac_apps: &["PhpStorm.app"] },
    BuiltinIde { id: "sublime",  name: "Sublime Text",    cmd: "subl",     args: &[],
                 mac_apps: &["Sublime Text.app"] },
    BuiltinIde { id: "rustrover", name: "RustRover",       cmd: "rustrover", args: &[],
                 mac_apps: &["RustRover.app"] },
    BuiltinIde { id: "vim",      name: "Vim",              cmd: "vim",      args: &[], mac_apps: &[] },
    BuiltinIde { id: "neovim",   name: "Neovim",           cmd: "nvim",     args: &[], mac_apps: &[] },
];

// ---------------------------------------------------------------------------
// Locating an IDE
// ---------------------------------------------------------------------------

/// Resolve `ide` to something launchable on this machine, or `None`.
///
/// Four steps, most authoritative first:
///   1. `cmd` as an **absolute path** that exists (a user's path override).
///   2. `PATH`.
///   3. The well-known launcher directories (`ide_bin_dirs`) — because a windowed app inherits the
///      system's minimal environment, not the user's shell profile, so `/usr/local/bin/code`,
///      `/opt/homebrew/bin/subl` and the JetBrains Toolbox scripts directory are all invisible to
///      step 2 even though they work perfectly in a terminal.
///   4. A macOS application bundle (`find_app_bundle`) — on a Mac this is the *normal* case, since
///      the CLI shims of steps 2–3 only exist if the user went looking for the setting that
///      installs them.
///
/// Steps 1–3 win over 4 when both exist: a shim honours the catalogue's `args` (VS Code's
/// `--new-window`), a bundle launch can't.
pub fn locate_ide(ide: &BuiltinIde, cmd: &str) -> Option<String> {
    let p = Path::new(cmd);
    if p.is_absolute() && p.exists() {
        return Some(cmd.to_string());
    }
    if let Some(hit) = find_on_path(cmd) {
        return Some(hit);
    }
    for dir in ide_bin_dirs() {
        if let Some(hit) = executable_in(&dir, cmd) {
            return Some(hit);
        }
    }
    find_app_bundle(ide.mac_apps)
}

/// `cmd` found in a `PATH` entry. An in-process scan rather than shelling out to `which` / `where`
/// once per catalogue entry — fifteen process spawns to answer a question that is fifteen `stat`s.
fn find_on_path(cmd: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| executable_in(&dir, cmd))
}

/// `cmd` (or, on Windows, `cmd` with each `PATHEXT` suffix) as a file directly in `dir`.
fn executable_in(dir: &Path, cmd: &str) -> Option<String> {
    let direct = dir.join(cmd);
    if direct.is_file() {
        return Some(direct.display().to_string());
    }
    if cfg!(windows) {
        // Most IDE launchers on Windows are `.cmd` / `.bat` shims, which a bare name never matches.
        for ext in ["exe", "cmd", "bat", "com"] {
            let candidate = dir.join(format!("{cmd}.{ext}"));
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

/// Directories that hold IDE launchers but are typically absent from a windowed app's `PATH`.
fn ide_bin_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![
        PathBuf::from("/usr/local/bin"),  // VS Code's `code` shim, manual installs
        PathBuf::from("/opt/homebrew/bin"), // Homebrew on Apple silicon
        PathBuf::from("/opt/local/bin"),  // MacPorts
        PathBuf::from("/snap/bin"),       // Linux snaps
    ];
    if let Some(home) = arbor_core::prelude::user_home() {
        out.push(home.join(".local/bin"));
        // JetBrains Toolbox writes one launcher script per installed IDE here — the only place an
        // `idea` / `rustrover` command exists for a Toolbox install, and never on a GUI `PATH`.
        out.push(home.join("Library/Application Support/JetBrains/Toolbox/scripts")); // macOS
        out.push(home.join("AppData/Local/JetBrains/Toolbox/scripts")); // Windows
        out.push(home.join(".local/share/JetBrains/Toolbox/scripts")); // Linux
    }
    out
}

/// The first of `names` that exists as a macOS application bundle, as an absolute path.
///
/// `~/Applications` is searched before `/Applications` because a per-user install (what JetBrains
/// Toolbox does by default) is the more specific answer. Always `None` off macOS — the parameter is
/// empty for terminal editors anyway, so this costs nothing there.
fn find_app_bundle(names: &[&str]) -> Option<String> {
    if !cfg!(target_os = "macos") || names.is_empty() {
        return None;
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = arbor_core::prelude::user_home() {
        roots.push(home.join("Applications"));
    }
    roots.push(PathBuf::from("/Applications"));
    // Toolbox can be configured to install into a subfolder.
    roots.push(PathBuf::from("/Applications/JetBrains Toolbox"));
    for root in roots {
        for name in names {
            let bundle = root.join(name);
            if bundle.is_dir() {
                return Some(bundle.display().to_string());
            }
        }
    }
    None
}

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
        spawn_ide_windows(path, ide_command, extra_args)
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        // A macOS application BUNDLE is a directory, not an executable — spawning it directly fails
        // with "permission denied". `open -a` is the platform's way to hand a folder to an app: it
        // reuses a running instance, detaches on its own, and works uniformly for VS Code, the
        // JetBrains IDEs, Sublime and Zed. The catalogue's `extra_args` are CLI-shim flags
        // (`--new-window`) that a bundle launch has no way to accept, so they're dropped here — a
        // detected shim still gets them, since a shim wins over a bundle in `locate_ide`.
        let (program, args): (&str, Vec<String>) = if ide_command.ends_with(".app") {
            ("open", vec!["-a".to_string(), ide_command.to_string(), path.to_string()])
        } else {
            let mut a: Vec<String> = extra_args.to_vec();
            a.push(path.to_string());
            (ide_command, a)
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(&args);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ide(id: &'static str, cmd: &'static str, mac_apps: &'static [&'static str]) -> BuiltinIde {
        BuiltinIde { id, name: id, cmd, args: &[], mac_apps }
    }

    /// Every catalogue entry that is a windowed IDE names at least one macOS bundle. Without one it
    /// is undetectable on a Mac unless the user installed its CLI shim by hand — which is the state
    /// the whole catalogue was in.
    #[test]
    fn every_gui_ide_names_a_mac_bundle() {
        const TERMINAL_ONLY: [&str; 2] = ["vim", "neovim"];
        for entry in BUILTIN_IDES {
            if TERMINAL_ONLY.contains(&entry.id) {
                assert!(entry.mac_apps.is_empty(), "{} is a terminal editor", entry.id);
                continue;
            }
            assert!(!entry.mac_apps.is_empty(), "{} has no macOS bundle to look for", entry.id);
            for app in entry.mac_apps {
                assert!(app.ends_with(".app"), "{app} is not a bundle name");
            }
        }
    }

    #[test]
    fn an_absolute_override_that_exists_wins() {
        // The test binary itself: an absolute path that is certainly a file.
        let me = std::env::current_exe().unwrap();
        let me = me.display().to_string();
        let found = locate_ide(&ide("x", "does-not-exist-anywhere", &[]), &me);
        assert_eq!(found.as_deref(), Some(me.as_str()));
    }

    #[test]
    fn an_absolute_override_that_does_not_exist_is_not_accepted() {
        let missing = if cfg!(windows) { "C:/nope/nothing.exe" } else { "/nope/nothing" };
        assert_eq!(locate_ide(&ide("x", "irrelevant", &[]), missing), None);
    }

    #[test]
    fn a_command_in_a_path_entry_is_found() {
        let dir = std::env::temp_dir().join(format!("arbor-ide-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) { "fake-ide.exe" } else { "fake-ide" };
        std::fs::write(dir.join(name), b"#!/bin/sh\n").unwrap();
        assert_eq!(
            executable_in(&dir, "fake-ide").as_deref(),
            Some(dir.join(name).display().to_string().as_str()),
            "a bare name resolves (with a PATHEXT suffix on Windows)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_command_resolves_to_nothing() {
        assert_eq!(locate_ide(&ide("x", "arbor-no-such-editor-xyz", &[]), "arbor-no-such-editor-xyz"), None);
    }

    /// A bundle path is what the launch has to recognise as needing `open -a`.
    #[test]
    fn bundle_paths_are_recognisable_by_suffix() {
        assert!("/Applications/Visual Studio Code.app".ends_with(".app"));
        assert!(!"/usr/local/bin/code".ends_with(".app"));
    }
}
