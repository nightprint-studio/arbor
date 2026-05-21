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
            r"/opt/microsoft/powershell/7/pwsh",
        ],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "bash",
        name: "Bash",
        cmd: "bash",
        args: &[],
        fallbacks: &["/bin/bash", "/usr/bin/bash"],
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
        fallbacks: &["/bin/zsh", "/usr/bin/zsh"],
        platforms: &["unix"],
    },
    BuiltinShell {
        id: "fish",
        name: "Fish",
        cmd: "fish",
        args: &[],
        fallbacks: &["/usr/bin/fish", "/usr/local/bin/fish"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "nushell",
        name: "Nushell",
        cmd: "nu",
        args: &[],
        fallbacks: &["/usr/bin/nu", "/usr/local/bin/nu"],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "xonsh",
        name: "Xonsh",
        cmd: "xonsh",
        args: &[],
        fallbacks: &[],
        platforms: &["any"],
    },
    BuiltinShell {
        id: "elvish",
        name: "Elvish",
        cmd: "elvish",
        args: &[],
        fallbacks: &[],
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
    if platforms.iter().any(|p| *p == "any") {
        return true;
    }
    #[cfg(target_os = "windows")]
    let host = "windows";
    #[cfg(not(target_os = "windows"))]
    let host = "unix";

    platforms.iter().any(|p| *p == host)
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
            let mut found = which_command(s.cmd);
            if found.is_none() {
                for fb in s.fallbacks {
                    if Path::new(fb).exists() {
                        found = Some((*fb).to_string());
                        break;
                    }
                }
            }
            DetectedShell {
                id:            s.id.to_string(),
                name:          s.name.to_string(),
                available:     found.is_some(),
                detected_path: found,
            }
        })
        .collect()
}

pub fn which_command(cmd: &str) -> Option<String> {
    #[cfg(windows)]
    let output = std::process::Command::new("where").arg(cmd).no_window().output();
    #[cfg(not(windows))]
    let output = std::process::Command::new("which").arg(cmd).output();

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
        return (platform_default().to_string(), Vec::new());
    }

    if let Some(custom) = cfg.custom_shells.iter().find(|s| s.id == id) {
        return (custom.command.clone(), custom.args.clone());
    }

    if let Some(builtin) = BUILTIN_SHELLS.iter().find(|s| s.id == id) {
        let exe = if let Some(ov) = cfg.path_overrides.get(id) {
            if !ov.is_empty() { ov.clone() } else { builtin.cmd.to_owned() }
        } else {
            builtin.cmd.to_owned()
        };
        let args = builtin.args.iter().map(|a| (*a).to_string()).collect();
        return (exe, args);
    }

    if id.contains(['/', '\\']) || id.ends_with(".exe") {
        return (id.to_string(), Vec::new());
    }

    (platform_default().to_string(), Vec::new())
}

pub fn platform_default() -> &'static str {
    #[cfg(target_os = "windows")]
    { "cmd.exe" }
    #[cfg(not(target_os = "windows"))]
    { "bash" }
}
