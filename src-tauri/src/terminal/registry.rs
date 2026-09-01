use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use crate::process_ext::NoWindowExt;

/// A built-in shell entry — known executable + display name.
pub struct BuiltinShell {
    pub id:         &'static str,
    pub name:       &'static str,
    /// Default command name probed via `which` / `where`.
    pub cmd:        &'static str,
    /// Extra arguments prepended on spawn (e.g. `-NoLogo` for PowerShell).
    pub args:       &'static [&'static str],
    /// Absolute paths probed when `cmd` is missing from PATH (Windows shells
    /// typically aren't on PATH after a default install, e.g. Git Bash).
    pub fallbacks:  &'static [&'static str],
    /// Which platforms this shell is shown on. `"any"` means all.
    pub platforms:  &'static [&'static str],
}

pub const BUILTIN_SHELLS: &[BuiltinShell] = &[
    BuiltinShell {
        id: "cmd",
        name: "Command Prompt",
        cmd: "cmd.exe",
        args: &[],
        fallbacks: &[r"C:\Windows\System32\cmd.exe"],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "powershell",
        name: "Windows PowerShell",
        cmd: "powershell.exe",
        args: &["-NoLogo"],
        fallbacks: &[r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "pwsh",
        name: "PowerShell 7+",
        cmd: "pwsh",
        args: &["-NoLogo"],
        fallbacks: &[
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            r"/usr/local/bin/pwsh",
            r"/opt/homebrew/bin/pwsh",
            r"/opt/microsoft/powershell/7/pwsh",
        ],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "bash",
        name: "Bash",
        cmd: "bash",
        args: &[],
        fallbacks: &["/bin/bash", "/usr/bin/bash", "/opt/homebrew/bin/bash"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "git-bash",
        name: "Git Bash",
        cmd: "git-bash",
        args: &["--login", "-i"],
        fallbacks: &[
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "wsl",
        name: "WSL",
        cmd: "wsl.exe",
        args: &[],
        fallbacks: &[r"C:\Windows\System32\wsl.exe"],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "msys2",
        name: "MSYS2",
        cmd: "msys2_shell.cmd",
        args: &[],
        fallbacks: &[
            r"C:\msys64\msys2_shell.cmd",
            r"C:\tools\msys64\msys2_shell.cmd",
        ],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "cygwin",
        name: "Cygwin",
        cmd: "cygwin.bat",
        args: &[],
        fallbacks: &[r"C:\cygwin64\Cygwin.bat", r"C:\cygwin\Cygwin.bat"],
        platforms: &["windows"],
    },
    BuiltinShell {
        id: "zsh",
        name: "Zsh",
        cmd: "zsh",
        args: &[],
        fallbacks: &["/bin/zsh", "/usr/bin/zsh", "/opt/homebrew/bin/zsh"],
        platforms: &["unix"],
    },
    BuiltinShell {
        id: "fish",
        name: "Fish",
        cmd: "fish",
        args: &[],
        fallbacks: &["/usr/bin/fish", "/usr/local/bin/fish", "/opt/homebrew/bin/fish"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "nushell",
        name: "Nushell",
        cmd: "nu",
        args: &[],
        fallbacks: &["/usr/bin/nu", "/usr/local/bin/nu", "/opt/homebrew/bin/nu"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "xonsh",
        name: "Xonsh",
        cmd: "xonsh",
        args: &[],
        fallbacks: &["/usr/local/bin/xonsh", "/opt/homebrew/bin/xonsh"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "elvish",
        name: "Elvish",
        cmd: "elvish",
        args: &[],
        fallbacks: &["/usr/local/bin/elvish", "/opt/homebrew/bin/elvish"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "tcsh",
        name: "tcsh",
        cmd: "tcsh",
        args: &[],
        fallbacks: &["/bin/tcsh", "/usr/bin/tcsh"],
        platforms: &["unix"],
    },
    BuiltinShell {
        id: "sh",
        name: "sh",
        cmd: "sh",
        args: &[],
        fallbacks: &["/bin/sh"],
        platforms: &["unix"],
    },
];

/// Returns true when the shell entry should be visible on the host platform.
pub fn shell_supports_host(platforms: &[&str]) -> bool {
    if platforms.contains(&"any") {
        return true;
    }
    #[cfg(target_os = "windows")]
    let host = "windows";
    #[cfg(not(target_os = "windows"))]
    let host = "unix";

    platforms.contains(&host)
}

/// Result of probing a single shell on the current system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedShell {
    pub id:            String,
    pub name:          String,
    /// True when the executable resolved (PATH or fallbacks or override).
    pub available:     bool,
    /// Resolved absolute executable path (None when not found).
    pub detected_path: Option<String>,
}

/// Probe every built-in shell that runs on the host platform and report which
/// ones are usable.  `path_overrides` maps shell id → custom executable path.
pub fn detect_available_shells(
    path_overrides: &HashMap<String, String>,
) -> Vec<DetectedShell> {
    BUILTIN_SHELLS
        .iter()
        .filter(|s| shell_supports_host(s.platforms))
        .map(|s| {
            if let Some(ov) = path_overrides.get(s.id) {
                if !ov.is_empty() {
                    let exists = Path::new(ov).exists() || which_command(ov).is_some();
                    return DetectedShell {
                        id:            s.id.to_string(),
                        name:          s.name.to_string(),
                        available:     exists,
                        detected_path: if exists { Some(ov.clone()) } else { None },
                    };
                }
            }
            let found = probe_executable(s.cmd, s.fallbacks);
            DetectedShell {
                id:            s.id.to_string(),
                name:          s.name.to_string(),
                available:     found.is_some(),
                detected_path: found,
            }
        })
        .collect()
}

/// PATH used when probing for shell executables.
///
/// A GUI process launched from Finder/Dock inherits launchd's environment, in
/// which `PATH` is only `/usr/bin:/bin:/usr/sbin:/sbin` — a `which` that
/// trusted it would report every Homebrew-installed shell as missing. The
/// usual install prefixes are *appended*, never prepended, so an entry the
/// user really has on PATH still wins.
#[cfg(not(windows))]
fn probe_path() -> String {
    const EXTRA: &[&str] = &[
        "/opt/homebrew/bin", // Homebrew, Apple Silicon
        "/usr/local/bin",    // Homebrew on Intel + most manual installs
        "/opt/local/bin",    // MacPorts
        "/usr/bin",
        "/bin",
    ];
    let mut parts: Vec<String> = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    for extra in EXTRA {
        if !parts.iter().any(|p| p == extra) {
            parts.push((*extra).to_string());
        }
    }
    parts.join(":")
}

/// Locate a shell executable: PATH first, then the entry's absolute fallbacks
/// (shells installed outside PATH — Git Bash on Windows, Homebrew formulae
/// under a GUI-inherited PATH). Shared by detection and spawn so the picker
/// can never advertise a shell the spawn path would then fail to find.
pub fn probe_executable(cmd: &str, fallbacks: &[&str]) -> Option<String> {
    if let Some(found) = which_command(cmd) {
        return Some(found);
    }
    fallbacks
        .iter()
        .find(|fb| Path::new(fb).exists())
        .map(|fb| (*fb).to_string())
}

pub fn which_command(cmd: &str) -> Option<String> {
    #[cfg(windows)]
    let output = std::process::Command::new("where").arg(cmd).no_window().output();
    #[cfg(not(windows))]
    let output = std::process::Command::new("which")
        .arg(cmd)
        .env("PATH", probe_path())
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        }
        _ => None,
    }
}

/// Resolve a shell id (or raw exec path) to (executable, args) using the
/// configured overrides + the built-in catalogue.  Falls back to the platform
/// default when `id` is empty or unknown.
pub fn resolve_shell(
    id_or_path: Option<&str>,
    cfg: &crate::config::app_config::TerminalsConfig,
) -> (String, Vec<String>) {
    let raw = id_or_path.unwrap_or("").trim();
    let id = if raw.is_empty() {
        cfg.default_shell.as_deref().unwrap_or("").trim()
    } else {
        raw
    };

    if id.is_empty() {
        return spawn_default();
    }

    // Custom shells are spelled out by the user, command and args both: adding
    // anything of ours would override an explicit choice.
    if let Some(custom) = cfg.custom_shells.iter().find(|s| s.id == id) {
        return (custom.command.clone(), custom.args.clone());
    }

    if let Some(builtin) = BUILTIN_SHELLS.iter().find(|s| s.id == id) {
        let exe = match cfg.path_overrides.get(id) {
            Some(ov) if !ov.is_empty() => ov.clone(),
            // Resolve to an absolute path with the same probe the picker used:
            // spawning by bare name would search the process PATH, which under
            // a GUI-inherited environment does not contain Homebrew.
            _ => probe_executable(builtin.cmd, builtin.fallbacks)
                .unwrap_or_else(|| builtin.cmd.to_owned()),
        };
        let mut args: Vec<String> = builtin.args.iter().map(|a| (*a).to_string()).collect();
        args.extend(login_args(&exe));
        return (exe, args);
    }

    if id.contains(['/', '\\']) || id.ends_with(".exe") {
        let args = login_args(id);
        return (id.to_string(), args);
    }

    spawn_default()
}

/// The default shell plus the flags it needs — the fallback of `resolve_shell`
/// in both the "nothing configured" and the "configured id is unknown" cases.
fn spawn_default() -> (String, Vec<String>) {
    let exe = platform_default();
    let args = login_args(&exe);
    (exe, args)
}

/// Flags that make a shell start as a **login** shell, keyed by its basename.
///
/// This is what repairs the environment on macOS. A GUI app inherits launchd's
/// `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`) and only a login shell rebuilds it:
/// `/etc/zprofile` runs `path_helper`, `~/.zprofile` runs `brew shellenv`, and
/// user toolchains live behind exactly those. `portable_pty` sets the login `-`
/// argv0 only for its own default program — a `CommandBuilder::new(prog)` gets
/// argv0 verbatim — and we cannot dash argv0 ourselves because it is also what
/// it resolves the executable from. So the flag is passed explicitly.
///
/// Empty on Windows, and for `sh`: that entry is the deliberate bare-shell
/// escape hatch, and loading a profile into it would defeat the point.
fn login_args(exe: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let _ = exe;
        Vec::new()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = Path::new(exe)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(exe);
        match base {
            "zsh" | "bash" | "fish" | "tcsh" | "csh" | "ksh" => vec!["-l".to_string()],
            _ => Vec::new(),
        }
    }
}

/// The shell a terminal opens when the user has not configured one.
///
/// On unix `$SHELL` is the user's own choice — set by the login session, and
/// by the passwd entry when the app is launched from Finder — so it is the
/// only honest default. Hard-coding `bash` handed macOS users a shell whose
/// rc files (`~/.bashrc`, `~/.bash_profile`) most of them do not even have,
/// which read as "the terminal has nothing in it".
pub fn platform_default() -> String {
    #[cfg(target_os = "windows")]
    {
        "cmd.exe".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| unix_baseline_shell().to_string())
    }
}

/// Fallback when `$SHELL` is unset — the platform's own stock login shell.
#[cfg(not(target_os = "windows"))]
fn unix_baseline_shell() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "/bin/zsh"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "/bin/bash"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn login_flag_is_keyed_on_the_basename_not_the_path() {
        assert_eq!(login_args("/opt/homebrew/bin/zsh"), vec!["-l".to_string()]);
        assert_eq!(login_args("zsh"), vec!["-l".to_string()]);
        assert_eq!(login_args("/bin/bash"), vec!["-l".to_string()]);
    }

    #[test]
    fn sh_and_non_posix_shells_stay_bare() {
        assert!(login_args("/bin/sh").is_empty());
        assert!(login_args("/opt/homebrew/bin/nu").is_empty());
        assert!(login_args("/usr/local/bin/pwsh").is_empty());
    }

    #[test]
    fn default_shell_is_a_login_shell() {
        let (exe, args) = spawn_default();
        assert!(!exe.is_empty());
        // Whatever `$SHELL` says, a POSIX shell must be asked to log in — that
        // is the whole point of the default path on macOS.
        if matches!(
            Path::new(&exe).file_name().and_then(|s| s.to_str()),
            Some("zsh" | "bash" | "fish")
        ) {
            assert_eq!(args, vec!["-l".to_string()]);
        }
    }

    #[test]
    fn builtin_shells_resolve_with_login_flags() {
        let cfg = crate::config::app_config::TerminalsConfig::default();
        let (exe, args) = resolve_shell(Some("zsh"), &cfg);
        assert!(exe.ends_with("zsh"), "unexpected exe: {exe}");
        assert_eq!(args, vec!["-l".to_string()]);

        let (_, sh_args) = resolve_shell(Some("sh"), &cfg);
        assert!(sh_args.is_empty());
    }

    #[test]
    fn probe_path_adds_the_install_prefixes_without_duplicating_them() {
        let path = probe_path();
        let entries: Vec<&str> = path.split(':').collect();
        let inherited: Vec<String> = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();

        for prefix in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            let was = inherited.iter().filter(|e| *e == prefix).count();
            let now = entries.iter().filter(|e| **e == prefix).count();
            // Present exactly once when we had to add it, and untouched when
            // the inherited PATH already carried it (however many times).
            assert_eq!(now, was.max(1), "{prefix} mishandled in {path}");
        }

        // Appended, never prepended — an inherited entry keeps its precedence.
        assert_eq!(entries[..inherited.len()], inherited[..]);
    }
}
