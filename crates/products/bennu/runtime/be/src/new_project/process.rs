//! Running one command for a wizard run: argv in, streamed lines out, killable.
//!
//! Three rules, each learned somewhere else in this backend:
//!
//! - **argv, never a shell string.** The program and every argument arrive as separate strings and
//!   go to `Command` as such. The values that reach here have already passed `names` / an
//!   allow-list, so there is nothing to quote — and nothing that could be interpreted.
//! - **stdin is null**, so a prompt fails at once instead of hanging a run nobody can answer
//!   (and so a child that touches its stdin's flags cannot touch the protocol pipe's).
//! - **cancel kills the tree.** `npm create` is `npm` → `node` → the scaffolder; killing the handle
//!   alone leaves the grandchildren writing into the folder the run is about to delete.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Cancellation shared between the run thread and the `cancel` handler.
#[derive(Default)]
pub struct Ctl {
    cancelled: AtomicBool,
    child: Mutex<Option<Arc<Mutex<Child>>>>,
}

impl Ctl {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// The raw flag, for code that polls it (the download loop).
    pub fn cancelled_flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    /// Mark the run cancelled and kill whatever it is running.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self.child.lock().ok().and_then(|g| g.clone()) {
            if let Ok(mut c) = child.lock() {
                crate::child::kill_tree(&mut c);
            }
        }
    }
}

/// How a command ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    Success,
    Cancelled,
    /// Non-zero exit, with the last lines of its output for the error message.
    Failed { code: Option<i32>, tail: String },
    /// Could not even start.
    NotStarted(String),
}

const TAIL_LINES: usize = 8;

/// Remove ANSI escape sequences (colours, cursor movement) from a line of tool output.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI: ESC [ params final-byte
            Some('[') => {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            // OSC: ESC ] … BEL or ESC \
            Some(']') => {
                chars.next();
                while let Some(n) = chars.next() {
                    if n == '\u{7}' || (n == '\u{1b}' && chars.peek() == Some(&'\\')) {
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

/// Run `argv` in `cwd`, calling `on_line` for each line of stdout/stderr as it arrives.
pub fn run(
    argv: &[String],
    cwd: &Path,
    ctl: &Arc<Ctl>,
    on_line: Arc<dyn Fn(&str) + Send + Sync>,
) -> Ended {
    let Some((program, rest)) = argv.split_first() else {
        return Ended::NotStarted("nothing to run".into());
    };
    // `Command::new("npm")` only tries `npm.exe` on Windows, and npm is a `.cmd` shim.
    let resolved = super::tools::locate(program).unwrap_or_else(|| program.clone());
    let mut cmd = Command::new(&resolved);
    cmd.args(rest)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Scaffolders and package managers check this to skip prompts and progress bars.
        .env("CI", "1")
        .env("NO_COLOR", "1");
    crate::child::own_group(&mut cmd);
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        cmd.no_window();
    }

    if ctl.is_cancelled() {
        return Ended::Cancelled;
    }
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ended::NotStarted(format!("`{program}` is not on your PATH"))
        }
        Err(e) => return Ended::NotStarted(format!("could not run `{program}`: {e}")),
    };
    crate::child::track(&child);
    let mut child = child;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let child = Arc::new(Mutex::new(child));
    if let Ok(mut slot) = ctl.child.lock() {
        *slot = Some(child.clone());
    }
    // A cancel that landed between the check above and the slot being filled.
    if ctl.is_cancelled() {
        ctl.cancel();
    }

    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let streams: Vec<Box<dyn std::io::Read + Send>> = [
        stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let readers: Vec<_> = streams
        .into_iter()
        .map(|stream| {
            let (on_line, tail) = (on_line.clone(), tail.clone());
            std::thread::spawn(move || read_lines(stream, &*on_line, &tail))
        })
        .collect();

    let status = loop {
        if let Ok(mut c) = child.lock() {
            match c.try_wait() {
                Ok(Some(st)) => break Some(st),
                Ok(None) => {}
                Err(_) => break None,
            }
        }
        std::thread::sleep(Duration::from_millis(60));
    };
    for r in readers {
        let _ = r.join();
    }
    crate::child::untrack(&child);
    if let Ok(mut slot) = ctl.child.lock() {
        *slot = None;
    }

    if ctl.is_cancelled() {
        return Ended::Cancelled;
    }
    match status {
        Some(st) if st.success() => Ended::Success,
        Some(st) => Ended::Failed { code: st.code(), tail: tail_text(&tail) },
        None => Ended::Failed { code: None, tail: tail_text(&tail) },
    }
}

fn read_lines<R: std::io::Read>(stream: R, on_line: &dyn Fn(&str), tail: &Mutex<Vec<String>>) {
    for raw in BufReader::new(stream).lines().map_while(Result::ok) {
        let line = strip_ansi(&raw);
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        on_line(line);
        if let Ok(mut t) = tail.lock() {
            t.push(line.to_string());
            if t.len() > TAIL_LINES {
                t.remove(0);
            }
        }
    }
}

fn tail_text(tail: &Mutex<Vec<String>>) -> String {
    tail.lock().map(|t| t.join("\n")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_is_stripped() {
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m done"), "ok done");
        assert_eq!(strip_ansi("\u{1b}[2K\u{1b}[1Gworking"), "working");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn a_missing_program_is_reported_not_panicked() {
        let ctl = Arc::new(Ctl::default());
        let out = run(
            &["definitely-not-a-real-program-xyz".to_string()],
            &std::env::temp_dir(),
            &ctl,
            Arc::new(|_| {}),
        );
        assert!(matches!(out, Ended::NotStarted(_)), "{out:?}");
    }

    #[test]
    fn empty_argv_does_not_start() {
        let ctl = Arc::new(Ctl::default());
        assert!(matches!(run(&[], &std::env::temp_dir(), &ctl, Arc::new(|_| {})), Ended::NotStarted(_)));
    }

    #[test]
    fn a_cancelled_run_never_spawns() {
        let ctl = Arc::new(Ctl::default());
        ctl.cancel();
        let out = run(&["git".to_string(), "--version".to_string()], &std::env::temp_dir(), &ctl, Arc::new(|_| {}));
        assert_eq!(out, Ended::Cancelled);
    }
}
