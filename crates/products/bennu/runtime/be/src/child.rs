//! Killing a spawned child **and everything it started**.
//!
//! Two functions that only work as a pair, shared by the domains that launch long-lived children
//! (`build`'s `java` run, `tests`' `mvn test`, `cargo_tests`' `cargo test`): [`own_group`] at spawn
//! time makes the child the root of something killable, and [`kill_tree`] kills it. Split across
//! two files they would drift, and the drift is silent — a child that outlives its Stop button
//! looks exactly like a child that stopped.

use std::process::{Child, Command};

#[cfg(windows)]
use arbor_process_ext::prelude::NoWindowExt;

/// Put a child at the head of its own process group, so [`kill_tree`] can take its descendants
/// with it. Call it on every `Command` whose output the console streams, **before** `spawn`.
///
/// ## Why a group and not just the handle
///
/// Because the handle is regularly not the process that matters. `mvn test` forks a **second** JVM
/// to run the tests in (Surefire's `forkCount`), and that JVM is a grandchild: killing Maven leaves
/// it running — holding `target/`, and, when the run was started under the debugger, sitting
/// suspended at a breakpoint with its JDWP socket still open. The editor then showed a debug
/// session that could not be ended: the current line stayed highlighted, the activity bar kept its
/// paused dot, and Stop had visibly done nothing.
///
/// Windows needs no equivalent — `taskkill /T` walks the real parent-child tree — so this is a
/// no-op there rather than a second mechanism to keep in step.
///
/// The cost is that the child no longer shares our process group, so a `SIGINT` sent to ours does
/// not reach it. Nothing here relies on that: every child is stopped explicitly, by id.
pub(crate) fn own_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 0 = "a new group, led by the child" — so its pgid is its own pid, which is what
        // `kill_tree` checks before signalling a group.
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

/// Kill `child` and its whole process tree.
///
/// `Child::kill` kills exactly the handle we hold, and that handle is regularly a **launcher** or a
/// parent: `mvn.cmd` on Windows, and on every platform the Maven JVM whose forked test JVM is where
/// the tests actually run. Killing it alone leaves the real process going — still holding
/// `target/`, still writing to files, still listening on the port — while the UI says the run has
/// stopped.
///
/// Two mechanisms, one per platform, because the platforms give different handles on the same
/// thing: `taskkill /T` walks the tree on Windows, and on Unix the child was made a **group leader**
/// by [`own_group`] so one `killpg` reaches every descendant that has not left the group.
///
/// `kill` follows in both cases: it reaps the handle so the waiting thread's `wait()` returns.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.arg("/PID").arg(child.id().to_string()).arg("/T").arg("/F");
        tk.no_window();
        let _ = tk.output();
    }
    #[cfg(unix)]
    {
        // Only when the child really leads a group of its own. A child spawned without
        // `own_group` shares OUR group, and its pid names either nothing or — one chance in a very
        // large number — somebody else's group: signalling that would kill an unrelated tree.
        // `getpgid(pid) == pid` is exactly the "we set this up" test, and it costs a syscall.
        let pid = child.id() as libc::pid_t;
        if pid > 0 && unsafe { libc::getpgid(pid) } == pid {
            unsafe { libc::killpg(pid, libc::SIGKILL) };
        }
    }
    let _ = child.kill();
}

/// What a streamed child left behind.
pub(crate) struct StreamedOutput {
    pub ok: bool,
    /// The last few lines, for a caller that has to say something short about a failure.
    /// The whole log already went to the panel while it ran.
    pub tail: String,
}

/// How much of the log a caller gets back. Enough for the sentence a package manager ends
/// with, short enough to fit in a toast.
const TAIL_LINES: usize = 12;

/// Run `argv` to completion, streaming its output into the **Build** panel line by line.
///
/// For the commands that are neither a build nor a run but take just as long — installing a
/// language server, in practice. They go to the build channel rather than growing a channel
/// of their own: the panel is already the place where "something long is happening and here
/// is what it is saying" lives, and a second one would be a second thing to go looking for.
///
/// Both streams are read, and interleaved as they arrive: `cargo` writes its progress to
/// stderr and a log that showed only stdout would be silent for the entire compile.
pub(crate) fn run_streamed(
    argv: &[String],
    sink: std::sync::Arc<dyn arbor_ipc::prelude::EventSink>,
    what: &str,
) -> Result<StreamedOutput, String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let (program, rest) = argv.split_first().ok_or("nothing to run")?;
    let mut cmd = Command::new(program);
    // **`stdin` must be null, not inherited.** A backend's fd 0 is the protocol pipe from the
    // shell, and an inherited one is the SAME open file description — so a child that sets
    // `O_NONBLOCK` on its stdin sets it on ours. Node does exactly that (libuv marks a pipe
    // non-blocking), which made `npm install -g …` from the Language Servers page end with
    // `serve loop ended with error: Resource temporarily unavailable` and the window reporting
    // *bennu-be disconnected* — an editor killed by its own install button.
    //
    // Nothing run here reads from stdin anyway: these are package-manager commands whose input
    // is their argv. Null rather than piped so a prompt fails immediately instead of hanging a
    // build nobody can answer.
    cmd.args(rest).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        cmd.no_window();
    }

    let emit = |line: &str| {
        sink.emit("arbor://bennu/build-output", serde_json::json!({ "text": line }));
    };
    emit(&format!("{what}: {}", argv.join(" ")));

    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        // The one failure worth naming: the package manager itself is missing, which is a
        // different problem from the install failing and has a different fix.
        std::io::ErrorKind::NotFound => format!("`{program}` is not on your PATH"),
        _ => format!("could not run `{program}`: {e}"),
    })?;

    let mut lines: Vec<String> = Vec::new();
    // stderr on this thread, stdout on another: a child that fills one pipe while nobody
    // reads the other blocks forever, and `cargo install` fills both.
    let out = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut collected = Vec::new();
        if let Some(out) = out {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                collected.push(line);
            }
        }
        collected
    });
    if let Some(err) = child.stderr.take() {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            emit(&line);
            lines.push(line);
        }
    }
    if let Ok(collected) = reader.join() {
        for line in collected {
            emit(&line);
            lines.push(line);
        }
    }

    let ok = child.wait().map(|s| s.success()).unwrap_or(false);
    emit(if ok { "Done." } else { "Failed." });
    let tail = lines
        .iter()
        .rev()
        .take(TAIL_LINES)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    Ok(StreamedOutput { ok, tail })
}
