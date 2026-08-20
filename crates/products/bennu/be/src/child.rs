//! Killing a spawned child **and everything it started**.
//!
//! One function, shared by the two domains that launch long-lived children (`build`'s
//! `java` run and `tests`' `mvn test`), because the Windows half of it is the kind of
//! detail that is right in the place someone remembered it and wrong everywhere else.

use std::process::Child;
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
use arbor_process_ext::prelude::NoWindowExt;

/// Kill `child` and its whole process tree.
///
/// `Child::kill` kills exactly the handle we hold, and on Windows that handle is usually a
/// **launcher**: `mvn.cmd` for a test run, and for anything started through a shell the real
/// work is a grandchild. Killing the launcher leaves the JVM running — still holding
/// `target/`, still writing to files, still listening on the port — while the UI says the run
/// has stopped. `taskkill /T` takes the tree.
///
/// `kill` follows in both cases: on Unix it is the whole mechanism, and everywhere it reaps
/// the handle so the waiting thread's `wait()` returns.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.arg("/PID").arg(child.id().to_string()).arg("/T").arg("/F");
        tk.no_window();
        let _ = tk.output();
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
    cmd.args(rest).stdout(Stdio::piped()).stderr(Stdio::piped());
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
