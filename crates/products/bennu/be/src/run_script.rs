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
    let interpreter = resolve_interpreter(&script)?;

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

fn resolve_interpreter(script: &Path) -> Result<Interpreter, String> {
    match extension_of(&script.to_string_lossy()).as_str() {
        "sh" | "bash" => bash_interpreter(),
        "bat" | "cmd" => {
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
        "ps1" => powershell_interpreter(),
        other => Err(format!("Bennu does not know how to run a `.{other}` file")),
    }
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
fn powershell_interpreter() -> Result<Interpreter, String> {
    let leading = vec![
        "-NoLogo".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
    ];
    if cfg!(windows) {
        return Ok(Interpreter { program: PathBuf::from("powershell.exe"), leading, label: "powershell" });
    }
    // Cross-platform PowerShell is `pwsh`, and it is not installed by default anywhere but
    // Windows — so this is a real question rather than a formality.
    if which_pwsh() {
        return Ok(Interpreter { program: PathBuf::from("pwsh"), leading, label: "pwsh" });
    }
    Err("`.ps1` needs PowerShell, and `pwsh` was not found on this machine. \
         Install PowerShell 7 (`brew install powershell` on macOS) to run it here."
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
