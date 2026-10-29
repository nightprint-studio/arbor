//! Managed processes: start a long-running command, get an id back at once, read its output later,
//! wait for it to be ready, stop it with its whole tree.
//!
//! One [`Manager`] per project. A process is a child plus two reader threads (stdout, stderr — both
//! must be drained or a chatty service blocks on a full pipe) and one supervisor thread that notices
//! the exit, enforces the lifetime cap and records how it ended. Nothing here touches an async
//! runtime, and nothing blocks the caller longer than the call asks to: `start` returns after the
//! spawn, `wait_ready` is the only call that waits, and it takes a cancel check.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;
use serde_json::json;

use crate::clock::now_ms;
use crate::logbuf::{LogBuffer, LogLine, LogQuery, Stream};
use crate::ready::{http_ok, port_open, Check, Readiness};
use crate::recipe::{resolve_cwd, Recipe};
use crate::timeline::{EventKind, Timeline};
use crate::tree::{kill_group_remnants, kill_tree, own_group, terminate_tree};

#[cfg(windows)]
use arbor_process_ext::prelude::NoWindowExt;

/// The caps that keep a forgotten service from becoming a leak.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Processes running at once.
    pub max_running: usize,
    /// Finished processes kept so their logs can still be read.
    pub max_retained: usize,
    /// How long a process may run unless its spec says otherwise.
    pub default_lifetime: Duration,
    pub log_lines: usize,
    pub log_bytes: usize,
    /// How long a polite stop waits before the tree is killed.
    pub stop_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_running: 8,
            max_retained: 24,
            default_lifetime: Duration::from_secs(30 * 60),
            log_lines: 5_000,
            log_bytes: 2 * 1024 * 1024,
            stop_grace: Duration::from_secs(3),
        }
    }
}

/// What to run.
#[derive(Debug, Clone)]
pub struct StartSpec {
    pub name: String,
    /// A shell command line (`cmd /C` on Windows, `sh -c` elsewhere).
    pub command: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// Ports it is expected to listen on: refused up front when one is already taken.
    pub ports: Vec<u16>,
    pub ready: Readiness,
    /// The recipe it belongs to, if any.
    pub group: Option<String>,
    pub lifetime: Option<Duration>,
    /// Who started it: a conversation id for the assistant, `None` for the user (the Live panel). Only the
    /// owner stops it, and nobody else may start a process with the same name or a declared port.
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Running,
    /// Running and its readiness checks passed.
    Ready,
    /// Ended by itself.
    Exited,
    /// Ended because we stopped it (a request, or the lifetime cap).
    Stopped,
}

impl Phase {
    pub fn is_finished(self) -> bool {
        matches!(self, Phase::Exited | Phase::Stopped)
    }
}

/// A process as the model and the panel see it.
#[derive(Debug, Clone, Serialize)]
pub struct ProcInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub pid: u32,
    pub phase: Phase,
    pub exit_code: Option<i32>,
    pub ready: bool,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub ports: Vec<u16>,
    pub group: Option<String>,
    /// The conversation that started it; `None` when the user did.
    pub owner: Option<String>,
    pub log_lines: usize,
    pub log_dropped: u64,
    /// Why it ended when that was not its own doing ("stopped on request", "max lifetime reached").
    pub note: Option<String>,
}

#[derive(Default)]
struct State {
    phase: Option<Phase>,
    exit_code: Option<i32>,
    ready: bool,
    ended_ms: Option<u64>,
    note: Option<String>,
}

struct Proc {
    id: String,
    spec: StartSpec,
    pid: u32,
    started: Instant,
    started_ms: u64,
    log: Mutex<LogBuffer>,
    child: Mutex<Child>,
    state: Mutex<State>,
    stop_requested: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Proc {
    fn phase(&self) -> Phase {
        let s = lock(&self.state);
        match s.phase {
            Some(p) => p,
            None if s.ready => Phase::Ready,
            None => Phase::Running,
        }
    }

    fn finished(&self) -> bool {
        self.phase().is_finished()
    }

    fn info(&self) -> ProcInfo {
        let s = lock(&self.state);
        let log = lock(&self.log);
        ProcInfo {
            id: self.id.clone(),
            name: self.spec.name.clone(),
            command: self.spec.command.clone(),
            cwd: self.spec.cwd.display().to_string(),
            pid: self.pid,
            phase: match s.phase {
                Some(p) => p,
                None if s.ready => Phase::Ready,
                None => Phase::Running,
            },
            exit_code: s.exit_code,
            ready: s.ready,
            started_ms: self.started_ms,
            ended_ms: s.ended_ms,
            ports: self.spec.ports.clone(),
            group: self.spec.group.clone(),
            owner: self.spec.owner.clone(),
            log_lines: log.len(),
            log_dropped: log.dropped(),
            note: s.note.clone(),
        }
    }

    /// The last `n` lines, joined, for an error message.
    fn tail_text(&self, n: usize) -> String {
        lock(&self.log).tail(n).iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

struct Inner {
    procs: Mutex<Vec<Arc<Proc>>>,
    limits: Limits,
    timeline: Arc<Timeline>,
    next_id: AtomicU64,
}

/// The processes of one project.
#[derive(Clone)]
pub struct Manager {
    inner: Arc<Inner>,
}

static ANSI: LazyLock<Regex> = LazyLock::new(|| Regex::new("\u{1b}\\[[0-9;?]*[A-Za-z]").expect("static regex"));

impl Manager {
    pub fn new(limits: Limits, timeline: Arc<Timeline>) -> Self {
        Self { inner: Arc::new(Inner { procs: Mutex::new(Vec::new()), limits, timeline, next_id: AtomicU64::new(1) }) }
    }

    pub fn timeline(&self) -> &Arc<Timeline> {
        &self.inner.timeline
    }

    pub fn limits(&self) -> &Limits {
        &self.inner.limits
    }

    // ── Start ────────────────────────────────────────────────────────────────────────────────────

    /// Spawn `spec` and return at once. Refused when the caps are reached, the name is already running,
    /// a declared port is already taken, or the working directory does not exist.
    pub fn start(&self, spec: StartSpec) -> Result<ProcInfo, String> {
        if spec.command.trim().is_empty() {
            return Err("the command is empty".into());
        }
        if !spec.cwd.is_dir() {
            return Err(format!("the working directory {} does not exist", spec.cwd.display()));
        }
        {
            let procs = lock(&self.inner.procs);
            let running: Vec<&Arc<Proc>> = procs.iter().filter(|p| !p.finished()).collect();
            if running.len() >= self.inner.limits.max_running {
                return Err(format!("{} processes are already running (the cap); stop one first", running.len()));
            }
            if let Some(same) = running.iter().find(|p| p.spec.name.eq_ignore_ascii_case(&spec.name)) {
                return Err(format!("`{}` is already running as {} — stop it first or use another name", spec.name, same.id));
            }
        }
        if let Some(p) = spec.ports.iter().find(|p| port_open(**p)) {
            return Err(format!("port {p} is already in use by something else; stop it or pick another port"));
        }

        let mut cmd = shell_command(&spec.command);
        cmd.current_dir(&spec.cwd).envs(spec.env.iter().cloned());
        // stdin null, not inherited: a backend's fd 0 is the protocol pipe, and a child that puts its
        // stdin in non-blocking mode (Node does) would break ours (see bennu-be's child.rs).
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        own_group(&mut cmd);
        #[cfg(windows)]
        cmd.no_window();

        let mut child = cmd.spawn().map_err(|e| format!("could not start `{}`: {e}", spec.command))?;
        let pid = child.id();
        // So the process-ext watchdog kills it if this backend dies without cleaning up.
        arbor_process_ext::prelude::track_child_group(pid);
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());

        let id = format!("p{}", self.inner.next_id.fetch_add(1, Ordering::SeqCst));
        let started_ms = now_ms();
        let proc = Arc::new(Proc {
            id: id.clone(),
            log: Mutex::new(LogBuffer::new(self.inner.limits.log_lines, self.inner.limits.log_bytes)),
            spec,
            pid,
            started: Instant::now(),
            started_ms,
            child: Mutex::new(child),
            state: Mutex::new(State::default()),
            stop_requested: AtomicBool::new(false),
        });
        {
            let mut procs = lock(&self.inner.procs);
            procs.push(Arc::clone(&proc));
            Self::trim_retained(&mut procs, self.inner.limits.max_retained);
        }
        lock(&proc.log).push(started_ms, Stream::System, &format!("$ {}   (cwd {})", proc.spec.command, proc.spec.cwd.display()));
        self.inner.timeline.push(
            EventKind::ProcessStarted,
            &proc.spec.name,
            Some(&id),
            None,
            &format!("started {} (pid {pid})", proc.spec.command),
            Some(json!({ "pid": pid, "group": proc.spec.group })),
        );
        if let Some(out) = stdout {
            self.pump(&proc, Stream::Stdout, out);
        }
        if let Some(err) = stderr {
            self.pump(&proc, Stream::Stderr, err);
        }
        self.supervise(&proc);
        Ok(proc.info())
    }

    /// Forget the oldest finished processes beyond the retention cap.
    fn trim_retained(procs: &mut Vec<Arc<Proc>>, max_retained: usize) {
        while procs.iter().filter(|p| p.finished()).count() > max_retained {
            if let Some(i) = procs.iter().position(|p| p.finished()) {
                procs.remove(i);
            }
        }
    }

    /// Copy one stream into the process's log and the timeline, line by line.
    fn pump(&self, proc: &Arc<Proc>, stream: Stream, src: impl Read + Send + 'static) {
        let (proc, timeline) = (Arc::clone(proc), Arc::clone(&self.inner.timeline));
        std::thread::spawn(move || {
            let mut reader = BufReader::new(src);
            let mut buf: Vec<u8> = Vec::new();
            loop {
                buf.clear();
                // Bounded read: a stream with no newline must not grow without limit.
                match reader.by_ref().take(64 * 1024).read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let raw = String::from_utf8_lossy(&buf);
                let text = ANSI.replace_all(raw.trim_end_matches(['\r', '\n']), "").to_string();
                lock(&proc.log).push(now_ms(), stream, &text);
                timeline.push(EventKind::Log, &proc.spec.name, Some(&proc.id), None, &text, Some(json!({ "stream": stream })));
            }
        });
    }

    /// Notice the exit, enforce the lifetime cap, record how it ended.
    fn supervise(&self, proc: &Arc<Proc>) {
        let (proc, inner) = (Arc::clone(proc), Arc::clone(&self.inner));
        std::thread::spawn(move || {
            let lifetime = proc.spec.lifetime.unwrap_or(inner.limits.default_lifetime);
            let mut capped = false;
            loop {
                std::thread::sleep(Duration::from_millis(80));
                let status = lock(&proc.child).try_wait();
                match status {
                    Ok(Some(st)) => {
                        arbor_process_ext::prelude::untrack_child_group(proc.pid);
                        Self::record_end(&inner, &proc, st.code());
                        return;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        Self::record_end(&inner, &proc, None);
                        return;
                    }
                }
                if !capped && proc.started.elapsed() >= lifetime {
                    capped = true;
                    lock(&proc.state).note = Some(format!("stopped: it reached its maximum lifetime of {} min", lifetime.as_secs() / 60));
                    proc.stop_requested.store(true, Ordering::SeqCst);
                    kill_tree(&mut lock(&proc.child));
                }
            }
        });
    }

    fn record_end(inner: &Inner, proc: &Proc, code: Option<i32>) {
        let stopped = proc.stop_requested.load(Ordering::SeqCst);
        let ended = now_ms();
        {
            let mut s = lock(&proc.state);
            s.phase = Some(if stopped { Phase::Stopped } else { Phase::Exited });
            s.exit_code = code;
            s.ended_ms = Some(ended);
            if stopped && s.note.is_none() {
                s.note = Some("stopped on request".into());
            }
        }
        let how = match (stopped, code) {
            (true, _) => "stopped".to_string(),
            (false, Some(c)) => format!("exited with code {c}"),
            (false, None) => "was terminated by a signal".to_string(),
        };
        lock(&proc.log).push(ended, Stream::System, &format!("[{how}]"));
        inner.timeline.push(
            EventKind::ProcessExited,
            &proc.spec.name,
            Some(&proc.id),
            None,
            &how,
            Some(json!({ "exit_code": code, "stopped": stopped })),
        );
    }

    // ── Read ─────────────────────────────────────────────────────────────────────────────────────

    pub fn list(&self) -> Vec<ProcInfo> {
        lock(&self.inner.procs).iter().map(|p| p.info()).collect()
    }

    pub fn running_count(&self) -> usize {
        lock(&self.inner.procs).iter().filter(|p| !p.finished()).count()
    }

    pub fn get(&self, id_or_name: &str) -> Result<ProcInfo, String> {
        self.find(id_or_name).map(|p| p.info())
    }

    pub fn logs(&self, id_or_name: &str, query: &LogQuery) -> Result<(ProcInfo, Vec<LogLine>), String> {
        let p = self.find(id_or_name)?;
        let lines = lock(&p.log).query(query);
        Ok((p.info(), lines))
    }

    /// Ports declared by processes that are running: what the HTTP tool may consider "its own".
    pub fn owned_ports(&self) -> Vec<u16> {
        lock(&self.inner.procs).iter().filter(|p| !p.finished()).flat_map(|p| p.spec.ports.clone()).collect()
    }

    /// The running process that already uses `name`, whoever started it.
    pub fn holder_of_name(&self, name: &str) -> Option<ProcInfo> {
        lock(&self.inner.procs).iter().filter(|p| !p.finished()).find(|p| p.spec.name.eq_ignore_ascii_case(name)).map(|p| p.info())
    }

    /// The running process that declared `port`, whoever started it.
    pub fn holder_of_port(&self, port: u16) -> Option<ProcInfo> {
        lock(&self.inner.procs).iter().filter(|p| !p.finished()).find(|p| p.spec.ports.contains(&port)).map(|p| p.info())
    }

    /// An id (`p3`) or a name; a name picks the newest process of that name, running ones first.
    fn find(&self, id_or_name: &str) -> Result<Arc<Proc>, String> {
        let procs = lock(&self.inner.procs);
        if let Some(p) = procs.iter().find(|p| p.id == id_or_name) {
            return Ok(Arc::clone(p));
        }
        let mut named: Vec<&Arc<Proc>> = procs.iter().filter(|p| p.spec.name.eq_ignore_ascii_case(id_or_name)).collect();
        named.sort_by_key(|p| (!p.finished(), p.started_ms));
        named
            .last()
            .map(|p| Arc::clone(p))
            .ok_or_else(|| format!("no process `{id_or_name}` (known: {})", procs.iter().map(|p| format!("{} [{}]", p.id, p.spec.name)).collect::<Vec<_>>().join(", ")))
    }

    // ── Ready ────────────────────────────────────────────────────────────────────────────────────

    /// Wait until `readiness` (or, if `None`, the one the process was started with) holds. Fails with the
    /// process's last output when it exits first or the timeout passes. `cancelled` is polled so a
    /// cancelled run does not sit here.
    pub fn wait_ready(&self, id_or_name: &str, readiness: Option<&Readiness>, cancelled: &dyn Fn() -> bool) -> Result<u64, String> {
        let p = self.find(id_or_name)?;
        let readiness = readiness.cloned().unwrap_or_else(|| p.spec.ready.clone());
        let begun = Instant::now();
        loop {
            if cancelled() {
                return Err("cancelled".into());
            }
            if p.finished() {
                let info = p.info();
                return Err(format!(
                    "`{}` {} before it was ready.\nLast output:\n{}",
                    p.spec.name,
                    match info.exit_code {
                        Some(c) => format!("exited with code {c}"),
                        None => "ended".to_string(),
                    },
                    p.tail_text(20)
                ));
            }
            let pending: Vec<String> = readiness.checks.iter().filter(|c| !self.check_holds(&p, c)).map(Check::describe).collect();
            if pending.is_empty() {
                let first = !std::mem::replace(&mut lock(&p.state).ready, true);
                if first {
                    self.inner.timeline.push(EventKind::ProcessReady, &p.spec.name, Some(&p.id), None, "ready", None);
                }
                return Ok(begun.elapsed().as_millis() as u64);
            }
            if begun.elapsed() >= readiness.timeout {
                return Err(format!(
                    "`{}` was not ready after {} s; still waiting for: {}.\nLast output:\n{}",
                    p.spec.name,
                    readiness.timeout.as_secs(),
                    pending.join("; "),
                    p.tail_text(20)
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn check_holds(&self, p: &Proc, check: &Check) -> bool {
        match check {
            Check::Port(port) => port_open(*port),
            Check::Log(re) => !lock(&p.log).query(&LogQuery { pattern: Some(re.clone()), stream: None, limit: Some(1), ..Default::default() }).is_empty(),
            Check::Http { url, status } => http_ok(url, *status),
            Check::Delay(d) => p.started.elapsed() >= *d,
        }
    }

    // ── Stop ─────────────────────────────────────────────────────────────────────────────────────

    /// Stop one process: ask politely, wait the grace period, then kill the whole tree.
    pub fn stop(&self, id_or_name: &str) -> Result<ProcInfo, String> {
        let p = self.find(id_or_name)?;
        self.stop_proc(&p);
        Ok(p.info())
    }

    fn stop_proc(&self, p: &Arc<Proc>) {
        if p.finished() {
            return;
        }
        p.stop_requested.store(true, Ordering::SeqCst);
        terminate_tree(&lock(&p.child));
        if !self.await_exit(p, self.inner.limits.stop_grace) {
            kill_tree(&mut lock(&p.child));
            self.await_exit(p, Duration::from_secs(3));
        }
        kill_group_remnants(p.pid);
    }

    fn await_exit(&self, p: &Proc, within: Duration) -> bool {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if p.finished() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        p.finished()
    }

    /// Stop every running process. All are asked at once, so the grace period is spent once, not per process.
    pub fn stop_all(&self) -> usize {
        let running: Vec<Arc<Proc>> = lock(&self.inner.procs).iter().filter(|p| !p.finished()).cloned().collect();
        for p in &running {
            p.stop_requested.store(true, Ordering::SeqCst);
            terminate_tree(&lock(&p.child));
        }
        let deadline = Instant::now() + self.inner.limits.stop_grace;
        while Instant::now() < deadline && running.iter().any(|p| !p.finished()) {
            std::thread::sleep(Duration::from_millis(40));
        }
        for p in &running {
            if !p.finished() {
                kill_tree(&mut lock(&p.child));
            }
        }
        for p in &running {
            self.await_exit(p, Duration::from_secs(3));
            kill_group_remnants(p.pid);
        }
        running.len()
    }

    /// Stop every running process that `owner` started (`None`: the user's own), leaving the others alone.
    pub fn stop_owned(&self, owner: Option<&str>) -> usize {
        let mine: Vec<Arc<Proc>> = lock(&self.inner.procs).iter().filter(|p| !p.finished() && p.spec.owner.as_deref() == owner).cloned().collect();
        for p in &mine {
            p.stop_requested.store(true, Ordering::SeqCst);
            terminate_tree(&lock(&p.child));
        }
        let deadline = Instant::now() + self.inner.limits.stop_grace;
        while Instant::now() < deadline && mine.iter().any(|p| !p.finished()) {
            std::thread::sleep(Duration::from_millis(40));
        }
        for p in &mine {
            if !p.finished() {
                kill_tree(&mut lock(&p.child));
            }
        }
        for p in &mine {
            self.await_exit(p, Duration::from_secs(3));
            kill_group_remnants(p.pid);
        }
        mine.len()
    }

    /// Kill everything at once, without the polite round. For the backend's own exit, where there is no time to be nice.
    pub fn kill_all_now(&self) {
        for p in lock(&self.inner.procs).iter().filter(|p| !p.finished()) {
            p.stop_requested.store(true, Ordering::SeqCst);
            kill_tree(&mut lock(&p.child));
        }
    }

    // ── Recipes ──────────────────────────────────────────────────────────────────────────────────

    /// Start a recipe's services in dependency order, waiting for each to be ready before the next.
    /// A service of the recipe that is already running is reused (and waited for), so running a
    /// recipe twice is harmless. If one fails, the ones this call started are stopped again; the
    /// failed one keeps its logs.
    pub fn start_recipe(&self, recipe: &Recipe, project_root: &std::path::Path, owner: Option<&str>, cancelled: &dyn Fn() -> bool) -> Result<Vec<ProcInfo>, String> {
        let mut started_here: Vec<String> = Vec::new();
        let mut infos = Vec::new();
        for service in recipe.start_order() {
            // Reused only when it is the same owner's: a service another owner holds is not ours to adopt.
            let existing = self
                .find(&service.name)
                .ok()
                .filter(|p| !p.finished() && p.spec.group.as_deref() == Some(recipe.name.as_str()) && p.spec.owner.as_deref() == owner);
            let id = match existing {
                Some(p) => p.id.clone(),
                None => {
                    let cwd = resolve_cwd(project_root, &service.cwd)?;
                    let spec = StartSpec {
                        name: service.name.clone(),
                        command: service.command.clone(),
                        cwd,
                        env: service.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                        ports: service.ports.clone(),
                        ready: service.ready.to_readiness(),
                        group: Some(recipe.name.clone()),
                        lifetime: service.max_minutes.map(|m| Duration::from_secs(m * 60)),
                        owner: owner.map(str::to_string),
                    };
                    match self.start(spec) {
                        Ok(info) => {
                            started_here.push(info.id.clone());
                            info.id
                        }
                        Err(e) => {
                            self.stop_ids(&started_here);
                            return Err(format!("{}: {e}", service.name));
                        }
                    }
                }
            };
            if let Err(e) = self.wait_ready(&id, None, cancelled) {
                self.stop_ids(&started_here.iter().filter(|s| **s != id).cloned().collect::<Vec<_>>());
                return Err(e);
            }
            infos.push(self.get(&id)?);
        }
        Ok(infos)
    }

    /// Stop the running services of a recipe, in its stop order.
    pub fn stop_recipe(&self, recipe: &Recipe) -> Vec<ProcInfo> {
        self.stop_recipe_where(recipe, |_| true)
    }

    /// Stop the running services of a recipe that `owner` started (`None`: the user's), leaving the ones
    /// another owner holds.
    pub fn stop_recipe_of(&self, recipe: &Recipe, owner: Option<&str>) -> Vec<ProcInfo> {
        self.stop_recipe_where(recipe, |p| p.spec.owner.as_deref() == owner)
    }

    fn stop_recipe_where(&self, recipe: &Recipe, wanted: impl Fn(&Proc) -> bool) -> Vec<ProcInfo> {
        let mut out = Vec::new();
        for service in recipe.stop_sequence() {
            if let Ok(p) = self.find(&service.name) {
                if !p.finished() && p.spec.group.as_deref() == Some(recipe.name.as_str()) && wanted(&p) {
                    self.stop_proc(&p);
                    out.push(p.info());
                }
            }
        }
        out
    }

    fn stop_ids(&self, ids: &[String]) {
        for id in ids {
            let _ = self.stop(id);
        }
    }
}

/// `command` as a shell invocation. On Windows the line goes to `cmd /C` verbatim (`raw_arg`), wrapped
/// in an extra pair of quotes: Rust's own argument quoting would mangle a command that already has quotes.
fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").raw_arg(format!("\"{command}\""));
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn manager() -> Manager {
        let limits = Limits { stop_grace: Duration::from_millis(600), ..Default::default() };
        Manager::new(limits, Arc::new(Timeline::new(1000)))
    }

    fn spec(name: &str, command: &str) -> StartSpec {
        StartSpec {
            name: name.into(),
            command: command.into(),
            cwd: std::env::temp_dir(),
            env: Vec::new(),
            ports: Vec::new(),
            ready: Readiness::default(),
            group: None,
            lifetime: None,
            owner: None,
        }
    }

    /// A command that prints a line and then lives for a while, on either OS.
    fn long_running(line: &str) -> String {
        if cfg!(windows) {
            format!("echo {line} & ping -n 30 127.0.0.1 > nul")
        } else {
            format!("echo {line}; sleep 30")
        }
    }

    fn wait_until(what: &str, f: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out waiting for {what}");
    }

    #[test]
    fn output_is_captured_and_the_exit_code_recorded() {
        let m = manager();
        let cmd = if cfg!(windows) { "echo hello & exit 3" } else { "echo hello; exit 3" };
        let info = m.start(spec("quick", cmd)).unwrap();
        wait_until("exit", || m.get(&info.id).unwrap().phase.is_finished());
        let (info, lines) = m.logs(&info.id, &LogQuery::default()).unwrap();
        assert_eq!(info.phase, Phase::Exited);
        assert_eq!(info.exit_code, Some(3));
        assert!(lines.iter().any(|l| l.text.trim() == "hello"), "{lines:?}");
        // The timeline saw the same line and the exit.
        let events = m.timeline().query(&Default::default());
        assert!(events.iter().any(|e| e.kind == EventKind::Log && e.text.trim() == "hello"));
        assert!(events.iter().any(|e| e.kind == EventKind::ProcessExited));
    }

    #[test]
    fn stop_ends_a_running_process_and_says_it_was_stopped() {
        let m = manager();
        let info = m.start(spec("svc", &long_running("up"))).unwrap();
        assert_eq!(info.phase, Phase::Running);
        wait_until("first line", || m.logs(&info.id, &LogQuery::default()).unwrap().1.iter().any(|l| l.text.trim() == "up"));
        let stopped = m.stop(&info.id).unwrap();
        assert_eq!(stopped.phase, Phase::Stopped);
        assert_eq!(m.running_count(), 0);
    }

    #[test]
    fn a_name_cannot_run_twice_and_the_cap_is_enforced() {
        let limits = Limits { max_running: 2, stop_grace: Duration::from_millis(300), ..Default::default() };
        let m = Manager::new(limits, Arc::new(Timeline::new(100)));
        m.start(spec("a", &long_running("a"))).unwrap();
        assert!(m.start(spec("A", &long_running("again"))).unwrap_err().contains("already running"));
        m.start(spec("b", &long_running("b"))).unwrap();
        assert!(m.start(spec("c", &long_running("c"))).unwrap_err().contains("the cap"));
        assert_eq!(m.stop_all(), 2);
        assert_eq!(m.running_count(), 0);
    }

    #[test]
    fn a_taken_port_and_a_missing_directory_are_refused_before_spawning() {
        let m = manager();
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let mut s = spec("p", &long_running("x"));
        s.ports = vec![port];
        assert!(m.start(s).unwrap_err().contains("already in use"));
        let mut s = spec("d", &long_running("x"));
        s.cwd = std::env::temp_dir().join("bennu-live-no-such-dir-xyz");
        assert!(m.start(s).unwrap_err().contains("does not exist"));
        assert!(m.list().is_empty());
    }

    #[test]
    fn readiness_by_log_line_succeeds_and_by_timeout_fails_with_the_output() {
        let m = manager();
        let info = m.start(spec("svc", &long_running("Started Application"))).unwrap();
        let ok = Readiness { checks: vec![Check::Log(Regex::new("Started .*").unwrap())], timeout: Duration::from_secs(8) };
        m.wait_ready(&info.id, Some(&ok), &|| false).unwrap();
        assert!(m.get(&info.id).unwrap().ready);
        assert_eq!(m.get(&info.id).unwrap().phase, Phase::Ready);

        let never = Readiness { checks: vec![Check::Log(Regex::new("never printed").unwrap())], timeout: Duration::from_millis(400) };
        let err = m.wait_ready(&info.id, Some(&never), &|| false).unwrap_err();
        assert!(err.contains("not ready after") && err.contains("never printed") && err.contains("Started Application"), "{err}");
        m.stop_all();
    }

    #[test]
    fn waiting_for_a_process_that_dies_reports_its_exit_and_its_output() {
        let m = manager();
        let cmd = if cfg!(windows) { "echo boom 1>&2 & exit 7" } else { "echo boom 1>&2; exit 7" };
        let info = m.start(spec("crash", cmd)).unwrap();
        let r = Readiness { checks: vec![Check::Port(1)], timeout: Duration::from_secs(5) };
        let err = m.wait_ready(&info.id, Some(&r), &|| false).unwrap_err();
        assert!(err.contains("exited with code 7") && err.contains("boom"), "{err}");
    }

    #[test]
    fn a_cancelled_wait_returns_at_once() {
        let m = manager();
        let info = m.start(spec("svc", &long_running("x"))).unwrap();
        let r = Readiness { checks: vec![Check::Port(1)], timeout: Duration::from_secs(60) };
        let t = Instant::now();
        assert_eq!(m.wait_ready(&info.id, Some(&r), &|| true).unwrap_err(), "cancelled");
        assert!(t.elapsed() < Duration::from_secs(2));
        m.stop_all();
    }

    #[test]
    fn a_process_past_its_lifetime_is_stopped_and_says_why() {
        let m = manager();
        let mut s = spec("short", &long_running("x"));
        s.lifetime = Some(Duration::from_millis(300));
        let info = m.start(s).unwrap();
        wait_until("lifetime stop", || m.get(&info.id).unwrap().phase.is_finished());
        let done = m.get(&info.id).unwrap();
        assert_eq!(done.phase, Phase::Stopped);
        assert!(done.note.unwrap().contains("maximum lifetime"));
    }

    #[test]
    fn ansi_colour_is_stripped_from_the_log() {
        assert_eq!(ANSI.replace_all("\u{1b}[31mred\u{1b}[0m plain", ""), "red plain");
    }

    #[test]
    fn a_name_finds_the_newest_process_of_that_name_and_an_unknown_one_lists_the_known() {
        let m = manager();
        let first = m.start(spec("svc", if cfg!(windows) { "exit 0" } else { "exit 0" })).unwrap();
        wait_until("first exit", || m.get(&first.id).unwrap().phase.is_finished());
        let second = m.start(spec("svc", &long_running("two"))).unwrap();
        assert_eq!(m.get("svc").unwrap().id, second.id);
        assert!(m.get("nope").unwrap_err().contains("known:"));
        m.stop_all();
    }

    #[test]
    fn stopping_by_owner_leaves_the_other_owners_processes_running() {
        let m = manager();
        let mine = m.start(StartSpec { owner: Some("c-1".into()), ports: vec![45871], ..spec("mine", &long_running("a")) }).unwrap();
        let theirs = m.start(StartSpec { owner: Some("c-2".into()), ..spec("theirs", &long_running("b")) }).unwrap();
        assert_eq!(mine.owner.as_deref(), Some("c-1"));
        assert_eq!(m.holder_of_name("MINE").unwrap().id, mine.id);
        assert_eq!(m.holder_of_port(45871).unwrap().id, mine.id);
        assert!(m.holder_of_port(45872).is_none());
        assert_eq!(m.stop_owned(Some("c-1")), 1);
        assert!(m.get(&mine.id).unwrap().phase.is_finished());
        assert!(!m.get(&theirs.id).unwrap().phase.is_finished(), "the other conversation's process survives");
        assert!(m.holder_of_name("mine").is_none(), "a stopped process holds nothing");
        m.stop_all();
    }

    #[test]
    fn kill_all_now_ends_everything_without_the_polite_round() {
        let m = manager();
        let info = m.start(spec("svc", &long_running("x"))).unwrap();
        m.kill_all_now();
        wait_until("kill", || m.get(&info.id).unwrap().phase.is_finished());
        assert_eq!(m.running_count(), 0);
    }
}
