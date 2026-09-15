//! The DTO Lab's JVM: one harness process per project, on the project's own JDK, started when first
//! asked and stopped once nobody has asked anything for a while.
//!
//! ## Why a process that stays up, and why not forever
//!
//! A JVM takes a second or two to start and load a project's validator, and generating the tests of
//! one class asks it one question per case. A JVM per question would make a class with thirty
//! constraints a minute's wait; one per project for the whole session would hold a JVM's worth of
//! memory for every project anybody once opened a DTO in. So a session lives until it has been idle
//! for as long as the settings allow (ten minutes by default), and a rebuilt project replaces the harness's class loader (a `classpath` request with
//! a new epoch) rather than the process.
//!
//! ## What a failure does to the session
//!
//! A reply with `ok: false` is an **answer** — the class did not load, the validator is not on the
//! classpath — and the session stays. No reply within [`REPLY_TIMEOUT`], or a process that exited,
//! **ends** it: a static initialiser waiting on a database will not finish, and the next question
//! deserves a fresh JVM rather than the same wait.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_dtolab::prelude::{decode_reply, encode_request, HARNESS_CLASS, HARNESS_SOURCE};
use serde_json::Value;

/// How long a session waits for its next question before its JVM is stopped — the user's setting,
/// read at every sweep, so a change applies to the sessions already running.
fn idle() -> Duration {
    let minutes = bennu_core::config::load().dtolab_idle_minutes.clamp(1, 240);
    Duration::from_secs(u64::from(minutes) * 60)
}
/// How long one answer may take.
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

struct Session {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
    next_id: u64,
    last_used: Instant,
    /// The classpath epoch the harness's class loader was built for.
    epoch: String,
    java: PathBuf,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Failure {
    /// The harness answered, and the answer is a failure. The session is fine.
    Answer(String),
    /// The session is gone or stuck.
    Lost(String),
}

fn sessions() -> &'static Mutex<HashMap<String, Arc<Mutex<Session>>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Session>>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ask the project's harness `op`, compiling the project first when it changed since its last compile.
pub(crate) fn call(ctx: &BennuState, root: &str, op: &str, args: &[&str]) -> Result<Value, String> {
    crate::build::ensure_compiled(ctx, root)?;
    ask(root, op, args)
}

/// [`call`] without the compile check — for the questions that follow one that already made it, so a
/// generation of fifty cases does not stat the source tree fifty times.
pub(crate) fn ask(root: &str, op: &str, args: &[&str]) -> Result<Value, String> {
    let java = java_executable(root);
    let session = session_for(root, &java)?;
    let (classpath, epoch) = crate::build::lab_classpath(root);
    let result = {
        let mut guard = session.lock().unwrap_or_else(|p| p.into_inner());
        let s = &mut *guard;
        s.last_used = Instant::now();
        let mut result = Ok(Value::Null);
        if s.epoch != epoch {
            result = request(s, "classpath", &[&epoch, &classpath]);
            if result.is_ok() {
                s.epoch = epoch;
            }
        }
        if result.is_ok() {
            result = request(s, op, args);
        }
        s.last_used = Instant::now();
        result
    };
    match result {
        Ok(body) => Ok(body),
        Err(Failure::Answer(message)) => Err(message),
        Err(Failure::Lost(message)) => {
            forget(root);
            Err(message)
        }
    }
}

fn session_for(root: &str, java: &Path) -> Result<Arc<Mutex<Session>>, String> {
    let mut map = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(existing) = map.get(root) {
        // A session busy answering someone else is alive by definition; only an idle one is looked at.
        let usable = match existing.try_lock() {
            Ok(mut s) => s.java == java && matches!(s.child.try_wait(), Ok(None)),
            Err(_) => true,
        };
        if usable {
            return Ok(Arc::clone(existing));
        }
        map.remove(root);
    }
    let session = Arc::new(Mutex::new(spawn(root, java)?));
    map.insert(root.to_string(), Arc::clone(&session));
    ensure_reaper();
    Ok(session)
}

fn spawn(root: &str, java: &Path) -> Result<Session, String> {
    let classes = harness_classes(java)?;
    let mut cmd = Command::new(java);
    cmd.arg("-Dfile.encoding=UTF-8")
        .arg("-cp")
        .arg(&classes)
        .arg(HARNESS_CLASS)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // The project's own output — a logger, a banner — lands in the backend log, where it can be
        // read when something did not work.
        .stderr(Stdio::inherit());
    cmd.no_window();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("The DTO Lab could not start {}: {e}", java.display()))?;
    let stdin = child.stdin.take().ok_or("The DTO Lab JVM has no input")?;
    let stdout = child.stdout.take().ok_or("The DTO Lab JVM has no output")?;
    let (sender, replies) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    Ok(Session {
        child,
        stdin,
        replies,
        next_id: 1,
        last_used: Instant::now(),
        epoch: String::new(),
        java: java.to_path_buf(),
    })
}

/// The directory holding the compiled harness for this JDK, compiling it on first use.
///
/// Compiled by the project's own `javac` rather than shipped as bytecode: a class file targets one
/// version, and the harness has to load on whatever JDK the project happens to use.
fn harness_classes(java: &Path) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    HARNESS_SOURCE.hash(&mut hasher);
    java.hash(&mut hasher);
    let dir = arbor_core::prelude::bennu_data_dir()
        .join("dtolab")
        .join("harness")
        .join(format!("{:016x}", hasher.finish()));
    if dir.join(format!("{HARNESS_CLASS}.class")).is_file() {
        return Ok(dir);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("The DTO Lab could not create {}: {e}", dir.display()))?;
    let source = dir.join(format!("{HARNESS_CLASS}.java"));
    std::fs::write(&source, HARNESS_SOURCE)
        .map_err(|e| format!("The DTO Lab could not write {}: {e}", source.display()))?;
    let javac = beside(java, "javac");
    let mut cmd = Command::new(&javac);
    cmd.arg("-encoding").arg("UTF-8").arg("-nowarn").arg("-d").arg(&dir).arg(&source);
    cmd.no_window();
    let output = cmd.output().map_err(|e| {
        format!("The DTO Lab needs a JDK, and {} could not be run: {e}", javac.display())
    })?;
    if !output.status.success() {
        let detail: Vec<String> =
            String::from_utf8_lossy(&output.stderr).lines().take(4).map(str::to_string).collect();
        return Err(format!(
            "The DTO Lab harness did not compile with {}: {}",
            javac.display(),
            detail.join(" ")
        ));
    }
    Ok(dir)
}

fn request(s: &mut Session, op: &str, args: &[&str]) -> Result<Value, Failure> {
    let id = s.next_id;
    s.next_id += 1;
    let line = encode_request(id, op, args);
    s.stdin
        .write_all(line.as_bytes())
        .and_then(|()| s.stdin.flush())
        .map_err(|e| Failure::Lost(format!("The DTO Lab JVM stopped: {e}")))?;
    loop {
        match s.replies.recv_timeout(REPLY_TIMEOUT) {
            Ok(text) => {
                let Some((reply_id, body)) = decode_reply(&text) else { continue };
                if reply_id != id {
                    continue;
                }
                if body.get("ok").and_then(Value::as_bool) == Some(true) {
                    return Ok(body);
                }
                let message = body.get("error").and_then(Value::as_str).unwrap_or("The DTO Lab harness failed");
                return Err(Failure::Answer(message.to_string()));
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err(Failure::Lost(format!(
                    "The DTO Lab JVM did not answer `{op}` within {} seconds — a static initialiser may be \
                     waiting on something that will not come. The next question starts a new JVM.",
                    REPLY_TIMEOUT.as_secs()
                )))
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Failure::Lost(
                    "The DTO Lab JVM exited; what it printed is in the backend log.".to_string(),
                ))
            }
        }
    }
}

fn forget(root: &str) {
    sessions().lock().unwrap_or_else(|p| p.into_inner()).remove(root);
}

fn ensure_reaper() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            reap();
        });
    });
}

/// Stop every session idle for longer than the setting allows. A session answering right now is left
/// alone.
fn reap() {
    // Read before taking the lock: it is a file read, and every other project waits on this map.
    let limit = idle();
    let mut map = sessions().lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, session| match session.try_lock() {
        Ok(s) => s.last_used.elapsed() < limit,
        Err(_) => true,
    });
}

/// The project's `java`: its JDK's when one is installed, else whatever `PATH` has.
fn java_executable(root: &str) -> PathBuf {
    crate::build::resolve_java_home(root)
        .map(|home| home.join("bin").join(executable("java")))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(executable("java")))
}

fn beside(java: &Path, tool: &str) -> PathBuf {
    match java.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => dir.join(executable(tool)),
        None => PathBuf::from(executable(tool)),
    }
}

fn executable(name: &str) -> String {
    match cfg!(windows) {
        true => format!("{name}.exe"),
        false => name.to_string(),
    }
}
