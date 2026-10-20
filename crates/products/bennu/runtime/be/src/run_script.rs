//! `run_script` domain — `bennu_run_script`: run a shell script straight from the editor.
//!
//! A project is not only its compiled code. The `deploy.sh` beside the pom, the `build.cmd` a
//! Windows colleague wrote, the `.ps1` that provisions a test database — these are read in the
//! editor and run in a terminal somewhere else, which is exactly the split the Run console exists
//! to close. Streamed through [`spawn_streamed`], so a script gets what a Java run already has:
//! its own console tab, live output, stdin, Stop, and a linkified stack trace when it prints one.
//!
//! ## The interpreter is the whole problem
//!
//! Three script kinds, and each one is available on a different set of machines:
//!
//! * **`.sh` / `.bash`** — everywhere on Unix. On Windows it needs a bash that is NOT the one in
//!   `System32`: that is the WSL launcher, and running a project script through it puts the script
//!   in a different filesystem with different paths and a different toolchain. Git for Windows
//!   ships the bash that means what the author meant, so that is the one this looks for.
//! * **`.bat` / `.cmd`** — Windows only, and not "mostly": a batch file is `cmd.exe` syntax and
//!   nothing else interprets it. Asked for elsewhere, this says so rather than finding something
//!   that will fail halfway through.
//! * **`.ps1`** — `powershell.exe` on Windows (with `-ExecutionPolicy Bypass`, because a script in
//!   your own project failing on an execution policy is a stop with no useful lesson in it), and
//!   PowerShell 7 (`pwsh`) elsewhere when it is installed.
//!
//! Every refusal names what was looked for and where, because "cannot run this" sends the reader
//! to their own machine and that is where the answer is.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::RunHandle;
use serde::Deserialize;

use crate::build::spawn_streamed;
use arbor_process_ext::prelude::NoWindowExt;

/// Args for [`bennu_run_script`].
#[derive(Deserialize)]
pub struct RunScriptArgs {
    /// Absolute path to the project root — what the console links stack traces against.
    pub root: String,
    /// Absolute path to the script to run.
    pub file: String,
    /// Arguments passed to the script.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory. Empty / absent = the script's own directory, which is what a script
    /// saying `./config` means.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Extra environment variables, merged over the inherited environment.
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// The shell to interpret the script with (`bash`, `zsh`, `sh`, `pwsh`, `powershell`, `cmd`).
    /// Empty / absent = chosen from the file's extension.
    #[serde(default)]
    pub shell: Option<String>,
}

/// Args for [`bennu_script_shells`].
#[derive(Deserialize)]
pub struct ScriptShellsArgs {
    /// The script (or just its name) — only the extension matters.
    pub file: String,
}

/// One shell the editor may offer for a script.
#[derive(serde::Serialize)]
pub struct ScriptShell {
    /// What `RunScriptArgs::shell` takes.
    pub id: &'static str,
    /// The name as people say it.
    pub label: &'static str,
    /// Where it was found, or why it was not.
    pub detail: String,
    /// Whether this machine has it. An unavailable shell is still listed: a configuration written
    /// on another machine names one, and hiding it would make the field read as empty.
    pub available: bool,
}

/// The shells that can interpret `file`, with what this machine has.
///
/// Filtered by the file's family on purpose: `bash` cannot run a `.ps1` and `cmd` cannot run a
/// `.sh`, so offering the whole list would be offering choices that can only fail. A `.bat` has one
/// interpreter and therefore no choice.
#[arbor_rpc::handler]
fn bennu_script_shells(_ctx: &BennuState, args: ScriptShellsArgs) -> Result<Vec<ScriptShell>, String> {
    Ok(shells_for(&extension_of(&args.file)))
}

fn shells_for(ext: &str) -> Vec<ScriptShell> {
    let found = |id, label, r: Result<Interpreter, String>| match r {
        Ok(i) => ScriptShell { id, label, detail: i.program.to_string_lossy().to_string(), available: true },
        Err(why) => ScriptShell { id, label, detail: why, available: false },
    };
    match ext {
        "sh" | "bash" => vec![
            found("bash", if cfg!(windows) { "Git Bash" } else { "bash" }, shell_interpreter("bash")),
            found("zsh", "zsh", shell_interpreter("zsh")),
            found("sh", "sh", shell_interpreter("sh")),
        ],
        "ps1" => vec![
            found("pwsh", "PowerShell 7", shell_interpreter("pwsh")),
            found("powershell", "Windows PowerShell", shell_interpreter("powershell")),
        ],
        "bat" | "cmd" => vec![found("cmd", "cmd", shell_interpreter("cmd"))],
        _ => Vec::new(),
    }
}

/// How a script kind is launched on this machine.
struct Interpreter {
    /// The program to spawn.
    program: PathBuf,
    /// Arguments that come BEFORE the script path (`/c`, `-File`, …).
    leading: Vec<String>,
    /// A word for the console header — `bash`, `cmd`, `powershell`.
    label: &'static str,
}

/// Run the script at `file`, streaming into the Run console.
///
/// Errors (never panics) when the file has no extension this understands, or when its interpreter
/// is not available on this machine — with a message that says which it was.
#[arbor_rpc::handler]
fn bennu_run_script(ctx: &BennuState, args: RunScriptArgs) -> Result<RunHandle, String> {
    let script = PathBuf::from(&args.file);
    if !script.is_file() {
        return Err(format!("{} is not a file", script.display()));
    }
    let interpreter = resolve_interpreter(&script, args.shell.as_deref())?;

    // The script's own directory unless told otherwise: a `deploy.sh` that reads `./env` means
    // the one beside it, not the one beside the pom.
    let cwd = match args.working_dir.as_deref() {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => script.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(&args.root)),
    };

    let mut cmd = Command::new(&interpreter.program);
    cmd.current_dir(&cwd);
    for a in &interpreter.leading {
        cmd.arg(a);
    }
    cmd.arg(script_arg(&script, interpreter.label));
    for a in &args.args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::piped());
    if let Some(env) = &args.env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    cmd.no_window();

    let name = script.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let command = display_command(&interpreter, &script, &args.args);
    let sink = ctx.event_sink();
    spawn_streamed(cmd, name, command, cwd.display().to_string(), &args.root, sink, |_| {}).map_err(
        |e| {
            format!(
                "Could not run {} ({}): {e}",
                script.display(),
                interpreter.program.to_string_lossy()
            )
        },
    )
}

/// How the script path is written on the command line. Git Bash takes a Unix-shaped path even on
/// Windows — `C:\p\x.sh` reaches it as an argument it half-understands, and the half that fails is
/// the drive letter reading as a hostname.
fn script_arg(script: &Path, label: &str) -> String {
    let text = script.display().to_string();
    if label != "bash" || !cfg!(windows) {
        return text;
    }
    let fwd = text.replace('\\', "/");
    // `C:/p/x.sh` → `/c/p/x.sh`, which is what a Git Bash script sees as its own path.
    let mut chars = fwd.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(drive), Some(':'), Some('/')) if drive.is_ascii_alphabetic() => {
            format!("/{}/{}", drive.to_ascii_lowercase(), &fwd[3..])
        }
        _ => fwd,
    }
}

/// The command line as the console prints it — the resolved interpreter, not the word the user
/// typed, because "which bash" is exactly the question a failing script raises on Windows.
fn display_command(interpreter: &Interpreter, script: &Path, args: &[String]) -> String {
    let mut parts = vec![interpreter.program.to_string_lossy().to_string()];
    parts.extend(interpreter.leading.iter().cloned());
    parts.push(script_arg(script, interpreter.label));
    parts.extend(args.iter().cloned());
    parts
        .into_iter()
        .map(|p| if p.contains(' ') { format!("\"{p}\"") } else { p })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Which extensions this can run at all — the FE asks the same question to decide whether to draw
/// a ▶, so the two lists have to agree.
pub fn is_runnable_script(file: &str) -> bool {
    matches!(extension_of(file).as_str(), "sh" | "bash" | "bat" | "cmd" | "ps1")
}

fn extension_of(file: &str) -> String {
    Path::new(file)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The interpreter for `script`: the one the user chose, else the one its extension implies.
///
/// A chosen shell must belong to the file's family — `bash` for a `.ps1` is refused with a sentence
/// rather than handed to a shell that would print a syntax error halfway through a deploy.
fn resolve_interpreter(script: &Path, chosen: Option<&str>) -> Result<Interpreter, String> {
    let ext = extension_of(&script.to_string_lossy());
    let default = match ext.as_str() {
        "sh" | "bash" => "bash",
        "bat" | "cmd" => "cmd",
        "ps1" => if cfg!(windows) { "powershell" } else { "pwsh" },
        other => return Err(format!("Bennu does not know how to run a `.{other}` file")),
    };
    let shell = chosen.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(default);
    if !shells_for(&ext).iter().any(|s| s.id == shell) {
        return Err(format!("`{shell}` cannot run a `.{ext}` file."));
    }
    shell_interpreter(shell)
}

/// Locate `id` on this machine. Every refusal says what was looked for and where.
fn shell_interpreter(id: &str) -> Result<Interpreter, String> {
    match id {
        "bash" => bash_interpreter(),
        "cmd" => {
            if !cfg!(windows) {
                return Err("A batch file is `cmd.exe` syntax, and only Windows has one. \
                            Nothing on this machine can run it."
                    .to_string());
            }
            Ok(Interpreter {
                program: PathBuf::from("cmd.exe"),
                leading: vec!["/c".to_string()],
                label: "cmd",
            })
        }
        "zsh" => posix_shell("zsh"),
        "sh" => posix_shell("sh"),
        "pwsh" | "powershell" => powershell_interpreter(id == "pwsh"),
        other => Err(format!("Unknown shell `{other}`")),
    }
}

/// `zsh` / `sh`: on PATH on Unix. On Windows `sh` ships next to Git Bash's `bash`, and `zsh` is not
/// something Windows has without WSL — which is deliberately not used, for the filesystem reason
/// the [`bash_interpreter`] docs spell out.
fn posix_shell(name: &'static str) -> Result<Interpreter, String> {
    if cfg!(windows) {
        if name == "sh" {
            for candidate in git_bash_candidates() {
                let sh = candidate.with_file_name("sh.exe");
                if sh.is_file() {
                    return Ok(Interpreter { program: sh, leading: Vec::new(), label: "bash" });
                }
            }
        }
        return Err(format!("`{name}` is not available on Windows outside WSL, which a project script should not run through."));
    }
    if on_path(name) {
        return Ok(Interpreter { program: PathBuf::from(name), leading: Vec::new(), label: name });
    }
    Err(format!("`{name}` was not found on PATH."))
}

/// Whether an executable named `name` exists in a `PATH` directory.
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

/// The bash to run a `.sh` with.
///
/// On Windows this deliberately does **not** take the first `bash.exe` on `PATH`: on a machine with
/// WSL that is `C:\Windows\System32\bash.exe`, the Linux launcher — the script would run against a
/// different filesystem, with `/mnt/c` paths and whatever toolchain that distribution has, which is
/// a spectacular way to fail a deploy script. Git for Windows' bash is the one whose view of the
/// project matches the editor's.
fn bash_interpreter() -> Result<Interpreter, String> {
    if !cfg!(windows) {
        return Ok(Interpreter { program: PathBuf::from("bash"), leading: Vec::new(), label: "bash" });
    }
    for candidate in git_bash_candidates() {
        if candidate.is_file() {
            return Ok(Interpreter { program: candidate, leading: Vec::new(), label: "bash" });
        }
    }
    Err("Windows has no bash of its own, and Git Bash was not found \
         (looked under Program Files and %LOCALAPPDATA%\\Programs\\Git). \
         Install Git for Windows to run `.sh` scripts. \
         The `bash.exe` in System32 is deliberately not used: it is the WSL launcher, and a script \
         run through it sees a different filesystem."
        .to_string())
}

/// Where Git for Windows puts its bash, in the order a machine is likely to have it.
fn git_bash_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        let Some(dir) = std::env::var_os(var) else { continue };
        let base = PathBuf::from(dir);
        // `LOCALAPPDATA` holds a per-user install one level deeper.
        let git = if var == "LOCALAPPDATA" { base.join("Programs").join("Git") } else { base.join("Git") };
        out.push(git.join("bin").join("bash.exe"));
        out.push(git.join("usr").join("bin").join("bash.exe"));
    }
    out
}

/// PowerShell: Windows' own, or PowerShell 7 where it is installed.
///
/// `-ExecutionPolicy Bypass` because the script being run is one the user opened in their own
/// project: the policy exists to stop scripts arriving from elsewhere, and refusing this one
/// teaches nothing that the file's presence in the tree has not already settled.
fn powershell_interpreter(want_pwsh: bool) -> Result<Interpreter, String> {
    let leading = vec![
        "-NoLogo".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
    ];
    if !want_pwsh {
        if cfg!(windows) {
            return Ok(Interpreter { program: PathBuf::from("powershell.exe"), leading, label: "powershell" });
        }
        return Err("Windows PowerShell (`powershell.exe`) exists only on Windows — use PowerShell 7."
            .to_string());
    }
    // Cross-platform PowerShell is `pwsh`, and it is not installed by default anywhere — so this is
    // a real question rather than a formality.
    if which_pwsh() {
        return Ok(Interpreter { program: PathBuf::from("pwsh"), leading, label: "pwsh" });
    }
    Err("`pwsh` was not found on this machine. Install PowerShell 7 \
         (`winget install Microsoft.PowerShell`, or `brew install powershell` on macOS)."
        .to_string())
}

/// Whether `pwsh` is on `PATH`. Asked by running it rather than by walking `PATH`, so a shim, an
/// alias directory or a Homebrew prefix nobody told us about all count.
fn which_pwsh() -> bool {
    let mut probe = Command::new("pwsh");
    probe.arg("-NoLogo").arg("-NoProfile").arg("-Command").arg("exit 0");
    probe.stdout(Stdio::null()).stderr(Stdio::null()).stdin(Stdio::null());
    probe.no_window();
    probe.status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FE draws its ▶ from the same predicate; a list that drifted would put an arrow on a
    /// file nothing can run, or withhold one from a file that runs fine.
    #[test]
    fn the_runnable_extensions_are_the_three_script_families() {
        for yes in ["/p/deploy.sh", "/p/build.BASH", "/p/x.bat", "/p/x.cmd", "/p/provision.ps1"] {
            assert!(is_runnable_script(yes), "{yes} should be runnable");
        }
        for no in ["/p/App.java", "/p/main.rs", "/p/notes.md", "/p/Makefile"] {
            assert!(!is_runnable_script(no), "{no} should not be runnable");
        }
    }

    /// A shell from the wrong family is refused with a sentence, not handed a file it would choke on.
    #[test]
    fn a_shell_must_belong_to_the_files_family() {
        let err = resolve_interpreter(Path::new("/p/provision.ps1"), Some("bash")).err().unwrap();
        assert!(err.contains("cannot run"), "{err}");
        let err = resolve_interpreter(Path::new("/p/x.sh"), Some("cmd")).err().unwrap();
        assert!(err.contains("cannot run"), "{err}");
        // Nothing but cmd runs a batch file, so the list has no choice to offer.
        assert_eq!(shells_for("bat").len(), 1);
        assert!(shells_for("md").is_empty());
    }

    /// Git Bash takes the script path in its own shape. A `C:\…` reaching it as an argument is
    /// read with the drive letter as a hostname, and the script "does not exist".
    #[test]
    fn a_windows_path_reaches_git_bash_unix_shaped() {
        // The conversion is `cfg!(windows)`-gated, so this asserts the shape only where it applies.
        if cfg!(windows) {
            assert_eq!(script_arg(Path::new(r"C:\p\deploy.sh"), "bash"), "/c/p/deploy.sh");
        }
        // Everywhere, a path handed to a non-bash interpreter is left exactly as it is.
        let native = Path::new("/p/x.ps1");
        assert_eq!(script_arg(native, "pwsh"), native.display().to_string());
    }
}
