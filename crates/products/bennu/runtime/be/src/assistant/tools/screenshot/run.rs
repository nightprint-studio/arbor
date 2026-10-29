//! Running a short-lived child with a deadline: capture its output, stop it when the time is up or the user
//! cancels — and take everything it started down with it.
//!
//! The browser and `node` are children of a conversation that can wait on them for a fraction of a minute at
//! most; neither may outlive the call. The process group / `taskkill /T` pairing in `crate::child` is what
//! makes "stop" mean stop for a browser, which spawns helper processes of its own.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished child left: whether it succeeded, and both streams.
pub(super) struct Captured {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Why a child did not finish normally.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Stopped {
    TimedOut,
    Cancelled,
    Spawn(String),
}

impl Stopped {
    pub(super) fn message(&self, what: &str, limit: Duration) -> String {
        match self {
            Stopped::TimedOut => format!("{what} did not finish within {} s and was stopped.", limit.as_secs()),
            Stopped::Cancelled => format!("{what} was stopped: the run was cancelled."),
            Stopped::Spawn(e) => format!("could not start {what}: {e}"),
        }
    }
}

/// How often the deadline and the cancel flag are looked at.
const POLL: Duration = Duration::from_millis(40);
/// The most of each stream kept: a browser can be chatty, and only the tail explains a failure.
const MAX_CAPTURE_BYTES: usize = 256 * 1024;

/// Run `cmd`, feeding `stdin_text` to it (if any), until it ends, `limit` passes or `cancelled()` is true.
pub(super) fn run(mut cmd: Command, stdin_text: Option<&str>, limit: Duration, cancelled: &dyn Fn() -> bool) -> Result<Captured, Stopped> {
    cmd.stdin(if stdin_text.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::child::own_group(&mut cmd);
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        cmd.no_window();
    }
    let mut child = cmd.spawn().map_err(|e| Stopped::Spawn(e.to_string()))?;
    crate::child::track(&child);

    // Written on its own thread: a script larger than the pipe buffer must not block before the child reads.
    if let (Some(text), Some(mut stdin)) = (stdin_text.map(str::to_string), child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => {}
            Err(e) => break Err(Stopped::Spawn(e.to_string())),
        }
        if cancelled() {
            break Err(Stopped::Cancelled);
        }
        if started.elapsed() >= limit {
            break Err(Stopped::TimedOut);
        }
        std::thread::sleep(POLL);
    };
    if status.is_err() {
        crate::child::kill_tree(&mut child);
    }
    arbor_process_ext::prelude::untrack_child_group(child.id());
    let _ = child.wait();
    let status = status?;
    Ok(Captured { ok: status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

/// Read a stream to its end on a thread, keeping the last [`MAX_CAPTURE_BYTES`].
fn drain<R: Read + Send + 'static>(stream: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut s) = stream {
            let mut buf = [0u8; 8192];
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&buf[..n]);
                if bytes.len() > 2 * MAX_CAPTURE_BYTES {
                    bytes.drain(..bytes.len() - MAX_CAPTURE_BYTES);
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command every platform has: it prints a word and ends.
    fn echo(word: &str) -> Command {
        if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", &format!("echo {word}")]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", &format!("echo {word}")]);
            c
        }
    }

    /// A command that outlives any test deadline.
    fn sleeper() -> Command {
        if cfg!(windows) {
            let mut c = Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        }
    }

    #[test]
    fn a_child_that_ends_gives_back_its_output() {
        let got = run(echo("hello"), None, Duration::from_secs(20), &|| false).ok().expect("it runs");
        assert!(got.ok && got.stdout.contains("hello"), "{}", got.stdout);
    }

    #[test]
    fn a_child_past_its_deadline_is_stopped() {
        let started = Instant::now();
        let got = run(sleeper(), None, Duration::from_millis(300), &|| false);
        assert!(matches!(got, Err(Stopped::TimedOut)));
        assert!(started.elapsed() < Duration::from_secs(10), "it was not left running");
    }

    #[test]
    fn a_cancelled_run_stops_the_child() {
        let got = run(sleeper(), None, Duration::from_secs(20), &|| true);
        assert!(matches!(got, Err(Stopped::Cancelled)));
    }

    #[test]
    fn a_program_that_does_not_exist_says_so() {
        let got = run(Command::new("definitely-not-a-program-arbor"), None, Duration::from_secs(5), &|| false);
        assert!(matches!(got, Err(Stopped::Spawn(_))));
    }

    #[test]
    fn the_messages_name_what_stopped() {
        let limit = Duration::from_secs(30);
        assert!(Stopped::TimedOut.message("the browser", limit).contains("30 s"));
        assert!(Stopped::Cancelled.message("node", limit).contains("cancelled"));
    }
}
