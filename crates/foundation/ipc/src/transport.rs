//! Framed-JSON transport over a duplex byte stream — the first **real** process
//! boundary for Model D.
//!
//! Stage 1 runs it over a child process's **stdin/stdout** (see
//! `docs/corvus-be-bringup.md`): the shell spawns `corvus-be` and frames
//! messages on its pipes; stderr stays free for logs. It's transport-agnostic by
//! construction — moving to a named pipe / unix socket later swaps the byte
//! stream under [`ChildClient`], not the protocol.
//!
//! - **Backend side**: [`serve_stdio`] runs the read→dispatch→reply loop and
//!   [`FrameEventSink`] is the [`EventSink`] that pushes `Event` frames.
//! - **Shell side**: [`ChildClient`] spawns the child, reads its `Hello`, demuxes
//!   replies/events on a reader thread, and implements [`BrokerClient`].

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{BrokerClient, Bytes};
use crate::error::IpcError;
use crate::event::EventSink;
use crate::host::HostCaller;

/// A shared, type-erased frame writer — the backend's stdout (the event sink and
/// the serve loop both write through it, serialized by the mutex).
pub type SharedWriter = Arc<Mutex<dyn Write + Send>>;

/// Correlates an in-flight request id to the channel that wakes its blocked
/// caller. Used by both sides: the shell's [`ChildClient`] (for `Response`s) and
/// the backend's [`FrameHostCaller`] (for `HostResponse`s).
type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>;

/// One length-prefixed JSON message on the duplex stream.
#[derive(Debug, Serialize, Deserialize)]
enum Frame {
    /// First frame the backend sends: the method names it serves (drives the
    /// shell's split routing).
    Hello { methods: Vec<String> },
    /// A call (shell → backend).
    Request {
        id: u64,
        method: String,
        params: Value,
    },
    /// The reply to a `Request` (backend → shell); `Err` carries the wire string.
    Response {
        id: u64,
        result: Result<Value, String>,
    },
    /// A push event (backend → shell), re-emitted to the FE by the shell.
    Event { topic: String, payload: Value },
    /// A reentrant call **backend → shell** (the reverse channel): the backend
    /// asks the shell for something only it can provide (a credential, a plugin
    /// UI round-trip) and blocks on the matching `HostResponse`. Separate `id`
    /// space from `Request` — each side mints its own ids.
    HostRequest {
        id: u64,
        method: String,
        params: Value,
    },
    /// The shell's reply to a `HostRequest`; `Err` carries the wire string.
    HostResponse {
        id: u64,
        result: Result<Value, String>,
    },
}

/// Fill `buf` completely. `Ok(false)` when the peer closed before a single byte arrived — a
/// clean end of stream — and an error when it closed part-way through, which is a truncated frame.
///
/// Not `Read::read_exact`, for one reason that costs a whole product window when it is missing:
/// that retries `Interrupted` and gives up on **`WouldBlock`**, and a backend's stdin can become
/// non-blocking without the backend doing anything. Inherited standard streams are the *same open
/// file description*, so a child process that sets `O_NONBLOCK` on its own stdin sets it on its
/// parent's — and Node does that to every pipe it is given. The reader then fails with
/// `Resource temporarily unavailable` on a stream that is perfectly healthy, the serve loop ends,
/// and the shell reports the backend as disconnected.
///
/// A child should not inherit this fd in the first place, and the fix belongs there. This is the
/// second line: waiting is what a blocking read would have done anyway, so the loop survives a
/// child that was rude once. The backoff is capped low enough to stay responsive and high enough
/// that a permanently poisoned stream idles at a few wake-ups a second rather than spinning.
fn read_all<R: Read + ?Sized>(r: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    let mut backoff = Duration::from_millis(1);
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => {
                return if filled == 0 {
                    Ok(false)
                } else {
                    Err(io::Error::from(io::ErrorKind::UnexpectedEof))
                };
            }
            Ok(n) => {
                filled += n;
                backoff = Duration::from_millis(1);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                warn_nonblocking_once();
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Say it once, on stderr, the first time a read finds the stream non-blocking.
///
/// Once because it would otherwise print several times a second, and at all because the cause is
/// always a bug somewhere else — some child was spawned inheriting this process's stdin — and
/// without a line here that bug is invisible: everything keeps working, slightly more expensively,
/// forever.
fn warn_nonblocking_once() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "[ipc] stdin turned non-blocking — some child process was spawned inheriting it.              Polling instead of failing; spawn children with `Stdio::null()` for stdin."
        );
    }
}

/// Read one frame, or `None` at clean EOF (peer closed the stream).
fn read_frame<R: Read + ?Sized>(r: &mut R) -> io::Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    if !read_all(r, &mut len_buf)? {
        return Ok(None);
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    read_all(r, &mut buf)?;
    let frame =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(frame))
}

/// Serialize + write one length-prefixed frame and flush.
fn write_frame<W: Write + ?Sized>(w: &mut W, frame: &Frame) -> io::Result<()> {
    let bytes =
        serde_json::to_vec(frame).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = (bytes.len() as u32).to_le_bytes();
    w.write_all(&len)?;
    w.write_all(&bytes)?;
    w.flush()
}

// ── Backend side ────────────────────────────────────────────────────────────

/// [`EventSink`] that pushes `Event` frames on the shared writer — the backend's
/// egress to the shell (which re-emits to the FE). In-process the shell uses a
/// different sink (`AppHandle::emit` directly); a split-out backend uses this.
pub struct FrameEventSink {
    out: SharedWriter,
}

impl FrameEventSink {
    pub fn new(out: SharedWriter) -> Self {
        Self { out }
    }
}

impl EventSink for FrameEventSink {
    fn emit(&self, topic: &str, payload: Value) {
        if let Ok(mut w) = self.out.lock() {
            let frame = Frame::Event { topic: topic.to_string(), payload };
            if let Err(e) = write_frame(&mut *w, &frame) {
                // stderr — stdout is the protocol channel.
                eprintln!("corvus-be: event emit failed: {e}");
            }
        }
    }
}

/// The backend's [`HostCaller`]: marshals a backend→shell call as a `HostRequest`
/// frame on the shared writer and blocks until the serve loop routes the matching
/// `HostResponse` back. The request/response twin of [`FrameEventSink`].
///
/// Its `pending` map is shared with [`serve_stdio`] (the reader that demuxes
/// incoming frames), so the reader can wake a blocked `call` — the reason the
/// serve loop must dispatch requests **off** the reader thread (see the deadlock
/// note in `docs/reverse-channel.md`).
pub struct FrameHostCaller {
    out: SharedWriter,
    pending: Pending,
    next_id: AtomicU64,
}

impl FrameHostCaller {
    pub fn new(out: SharedWriter) -> Arc<Self> {
        Arc::new(Self {
            out,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        })
    }

    /// The shared pending map — [`serve_stdio`] routes `HostResponse`s through it.
    fn pending(&self) -> Pending {
        Arc::clone(&self.pending)
    }
}

impl HostCaller for FrameHostCaller {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "host pending poisoned".to_string())?
            .insert(id, tx);
        {
            let mut w = self.out.lock().map_err(|_| "host writer poisoned".to_string())?;
            write_frame(&mut *w, &Frame::HostRequest { id, method: method.to_string(), params })
                .map_err(|e| e.to_string())?;
        }
        match rx.recv() {
            Ok(result) => result,
            Err(_) => Err("shell disconnected".to_string()),
        }
    }
}

/// The most handlers a backend runs at once. Past this, requests queue and are served in order.
///
/// High enough that it is not a throughput limit — a handler can legitimately take minutes (a Maven
/// build, an index rebuild, a query) and must not hold up the rest, and one blocked on the reverse
/// channel waiting for a credential holds a slot for as long as the person takes to answer. Low
/// enough to bound a stampede: see [`DispatchPool`].
const MAX_WORKERS: usize = 64;

/// How long a worker waits for something to do before giving its thread back. Long enough that a
/// working session reuses the same threads, short enough that an idle backend costs nothing.
const WORKER_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

type Job = Box<dyn FnOnce() + Send + 'static>;

/// The pool the serve loop runs handlers on: reuse an idle thread, spawn one when there is none,
/// stop at [`MAX_WORKERS`].
///
/// Dispatch has to stay **off** the reader thread (a handler that calls back to the shell blocks
/// until the reader delivers its `HostResponse`) and it has to stay **concurrent** (a slow handler
/// must not hold up the next one). Both were true of the thread-per-request it replaces. What that
/// could not do is refuse to scale with the size of a burst.
///
/// And bursts are real. The frontend's webview is power-throttled by the OS while its window is in
/// the background while the backend feeding it is not, so a backlog of events builds up and is
/// delivered all at once the moment the window regains focus — thousands of handlers in a few
/// milliseconds, each of them an OS thread, all contending for the same state mutex. The backend
/// then answers *nothing*, which from every other side looks like unrelated domains timing out and
/// is miserable to attribute. The frontend no longer produces that burst (its event streams are
/// coalesced), but a backend that falls over when someone does is a backend with a loaded gun in
/// it; this is the safety catch, not the fix.
struct DispatchPool {
    tx: mpsc::Sender<Job>,
    /// Shared by every worker: whoever holds the lock is the one waiting for the next job.
    rx: Arc<Mutex<mpsc::Receiver<Job>>>,
    /// Workers waiting for work (including those queued behind `rx`'s lock). Non-zero means the
    /// next job will be picked up without spawning anything.
    idle: Arc<AtomicUsize>,
    /// Workers that exist. Bounded by [`MAX_WORKERS`].
    alive: Arc<AtomicUsize>,
}

/// Keeps `alive` honest however a worker leaves — including a panic that escaped
/// [`dispatch_caught`], which would otherwise leak a slot out of the cap for the process's life.
struct WorkerSlot(Arc<AtomicUsize>);

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl DispatchPool {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
            idle: Arc::new(AtomicUsize::new(0)),
            alive: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn execute(&self, job: Job) {
        // Grow only when nobody is already waiting to take this. The check races with a worker
        // going idle, and benignly: the worst outcome is one more thread than strictly needed,
        // which the idle timeout reclaims.
        if self.idle.load(Ordering::Acquire) == 0 && self.alive.load(Ordering::Acquire) < MAX_WORKERS
        {
            self.spawn_worker();
        }
        // The receiver lives in `self`, so this cannot fail while the pool does. If it somehow
        // does, run the job here rather than drop it: a dropped request is a caller blocked on a
        // reply that will never come, which is the one outcome this whole file exists to prevent.
        if let Err(returned) = self.tx.send(job) {
            (returned.0)();
        }
    }

    fn spawn_worker(&self) {
        let rx = Arc::clone(&self.rx);
        let idle = Arc::clone(&self.idle);
        let alive = Arc::clone(&self.alive);
        alive.fetch_add(1, Ordering::AcqRel);
        let spawned = thread::Builder::new()
            .name("arbor-ipc-dispatch".to_string())
            .spawn(move || {
                let _slot = WorkerSlot(alive);
                loop {
                    idle.fetch_add(1, Ordering::AcqRel);
                    let job = {
                        let guard = rx.lock().unwrap_or_else(|e| e.into_inner());
                        guard.recv_timeout(WORKER_IDLE_TIMEOUT)
                    };
                    idle.fetch_sub(1, Ordering::AcqRel);
                    match job {
                        Ok(job) => job(),
                        // Idle for long enough to be worth giving the thread back — or the serve
                        // loop is gone and the sender with it.
                        Err(_) => break,
                    }
                }
            });
        if spawned.is_err() {
            // Nothing was spawned, so nothing will drop the slot.
            self.alive.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// Run the backend serve loop: announce `methods` via `Hello`, run `on_ready`
/// (post-Hello startup work — see below), then read frames
/// from `input` (the backend's stdin). Each `Request` is dispatched on a
/// [`DispatchPool`] **worker thread** (so the reader stays free to receive
/// `HostResponse`s while a handler is mid-`HostRequest` — the reverse-channel
/// reentrancy requirement); the worker writes its `Response` on `out` when done.
/// `HostResponse`s are routed back to the matching blocked
/// [`FrameHostCaller::call`] via `host`'s shared pending map.
///
/// `dispatch` must be `Send + Sync + 'static` because it runs on worker threads;
/// handlers therefore run **concurrently** (the single-threaded loop's implicit
/// serialization is gone — backend state is `Mutex`-guarded, matching the
/// in-process `LoopbackBroker`, which is already called concurrently).
///
/// Returns when the peer closes `input` (the shell exited).
///
/// ## A panicking handler answers
///
/// Every request is answered, including one whose handler panicked — see
/// [`dispatch_caught`]. A worker that unwinds past the write is a request that is
/// never replied to, and the caller on the other side is blocked on a channel with
/// no timeout: the window hangs on "reading schema…" forever, with no error in the
/// frontend, none in the backend's reply, and only a line on stderr nobody is
/// watching. Turning that into a legible message is the difference between a bug
/// with a name and an application that stopped working for no reason.
pub fn serve_stdio<R, F, I>(
    input: R,
    out: SharedWriter,
    methods: Vec<String>,
    host: Arc<FrameHostCaller>,
    dispatch: F,
    on_ready: I,
) -> io::Result<()>
where
    R: Read,
    F: Fn(&str, Value) -> Result<Value, String> + Send + Sync + 'static,
    I: FnOnce(),
{
    {
        let mut w = out.lock().expect("frame writer poisoned");
        write_frame(&mut *w, &Frame::Hello { methods })?;
    }

    // Post-Hello startup hook. The shell's handshake reads the FIRST frame and
    // requires it to be `Hello`; any `Event` frame emitted before this point
    // (e.g. by plugin on-load hooks) would race ahead of `Hello` on the pipe and
    // break the connection ("backend did not open with Hello"). Running such work
    // here guarantees it happens strictly AFTER `Hello` is on the wire — the read
    // loop below hasn't started yet, so no request can be dispatched mid-hook.
    on_ready();

    let dispatch = Arc::new(dispatch);
    let pending = host.pending();
    let pool = DispatchPool::new();
    let mut reader = input;
    while let Some(frame) = read_frame(&mut reader)? {
        match frame {
            Frame::Request { id, method, params } => {
                // Dispatch off the reader thread so a handler that calls back to
                // the shell (and blocks) doesn't stall the reader that must
                // deliver its `HostResponse`.
                let out = Arc::clone(&out);
                let dispatch = Arc::clone(&dispatch);
                pool.execute(Box::new(move || {
                    let result = dispatch_caught(&*dispatch, &method, params);
                    if let Ok(mut w) = out.lock() {
                        let _ = write_frame(&mut *w, &Frame::Response { id, result });
                    }
                }));
            }
            Frame::HostResponse { id, result } => {
                if let Some(tx) = pending.lock().expect("host pending poisoned").remove(&id) {
                    let _ = tx.send(result);
                }
            }
            // Hello/Response/Event/HostRequest are not expected shell → backend.
            _ => {}
        }
    }

    // Stream closed: fail any in-flight host-calls so blocked handlers unwind
    // instead of hanging forever.
    for (_, tx) in pending.lock().expect("host pending poisoned").drain() {
        let _ = tx.send(Err("shell disconnected".to_string()));
    }
    Ok(())
}

/// Run one dispatch, converting a panic into the error the caller gets back.
///
/// `AssertUnwindSafe` is the honest annotation rather than a shrug: the dispatch
/// closure owns state that a panic may leave half-written, and this does not claim
/// otherwise. What it claims is narrower and true — that answering is better than
/// not answering. The alternative on this path is not a clean state, it is a caller
/// blocked forever on a reply that will never come.
///
/// The panic's own message is forwarded. It is the only description of what went
/// wrong that exists, it names the file and line, and the caller is the one person
/// in a position to report it.
fn dispatch_caught<F>(dispatch: &F, method: &str, params: Value) -> Result<Value, String>
where
    F: Fn(&str, Value) -> Result<Value, String> + ?Sized,
{
    let caught =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(method, params)));
    match caught {
        Ok(result) => result,
        Err(payload) => {
            let what = panic_message(&payload);
            // stderr, because stdout is the protocol channel.
            eprintln!("arbor-ipc: handler `{method}` panicked: {what}");
            Err(format!("`{method}` failed with an internal error: {what}"))
        }
    }
}

/// The text of a caught panic, for the two shapes `panic!` actually produces.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "panic with no message".to_string()
}

// ── Shell side ──────────────────────────────────────────────────────────────

/// A [`BrokerClient`] backed by a spawned child process, framed over its stdio.
///
/// A background thread reads the child's stdout and demuxes: `Response` frames
/// wake the matching blocked [`call`](BrokerClient::call); `Event` frames go to
/// the `on_event` callback (the shell re-emits them to the FE); `HostRequest`
/// frames (the reverse channel) go to `host_dispatch`, whose result is written
/// back as a `HostResponse`. The reader thread is already independent of any
/// blocked `call`, so backend→shell requests are handled reentrantly. The child
/// is killed when this client drops.
pub struct ChildClient {
    inner: Arc<ChildInner>,
}

struct ChildInner {
    /// Child stdin — where requests + host-replies are written (the reader thread
    /// also writes `HostResponse`s here, so it's the shared [`SharedWriter`]).
    writer: SharedWriter,
    pending: Pending,
    next_id: AtomicU64,
    /// The executable's file name, purely so a failure can say which backend it
    /// was. Every client used to report `corvus-be`, which on a Picus or a Merula
    /// window is not a detail that is slightly off — it is the wrong answer to the
    /// first question anybody debugging asks.
    program: String,
    /// Kept so the child is killed on drop (closing the pipes alone leaves it).
    child: Mutex<Option<Child>>,
}

impl ChildClient {
    /// Spawn `cmd` (its stdin/stdout are overridden to pipes; stderr is left as
    /// configured), read the backend's `Hello`, and start the reader thread.
    /// Returns the client plus the method names the backend advertised.
    ///
    /// `on_event` is invoked for every push event the backend emits;
    /// `host_dispatch` answers every backend-originated `HostRequest` (the
    /// reverse channel — credential resolution, plugin-UI round-trips);
    /// `on_disconnect` fires **once** when the backend's stream closes (the
    /// process died or a framing error broke the channel), after every in-flight
    /// call has been failed — the shell uses it to surface a fatal "backend
    /// stopped" state rather than letting each later call fail piecemeal.
    pub fn spawn<E, H, D>(mut cmd: Command, on_event: E, host_dispatch: H, on_disconnect: D) -> io::Result<(Self, Vec<String>)>
    where
        E: Fn(String, Value) + Send + 'static,
        H: Fn(&str, Value) -> Result<Value, String> + Send + 'static,
        D: Fn() + Send + 'static,
    {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let program = std::path::Path::new(cmd.get_program())
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "backend".to_string());
        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("child stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("child stdout missing"))?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let writer: SharedWriter = Arc::new(Mutex::new(stdin));
        let inner = Arc::new(ChildInner {
            writer: Arc::clone(&writer),
            pending: Arc::clone(&pending),
            next_id: AtomicU64::new(1),
            program: program.clone(),
            child: Mutex::new(Some(child)),
        });

        // Read the Hello synchronously so the caller gets the method set up front.
        let mut reader = io::BufReader::new(stdout);
        let methods = match read_frame(&mut reader)? {
            Some(Frame::Hello { methods }) => methods,
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "backend did not open with Hello",
                ))
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "backend closed before Hello",
                ))
            }
        };

        // Demux replies + events + reverse-channel requests until the child
        // closes its stdout. This thread is independent of any blocked `call`, so
        // a `HostRequest` is served even while a forward call is in flight.
        let writer_for_reader = Arc::clone(&writer);
        thread::spawn(move || {
            let mut reader = reader;
            loop {
                match read_frame(&mut reader) {
                    Ok(Some(Frame::Response { id, result })) => {
                        if let Some(tx) = pending.lock().expect("pending poisoned").remove(&id) {
                            let _ = tx.send(result);
                        }
                    }
                    Ok(Some(Frame::Event { topic, payload })) => on_event(topic, payload),
                    Ok(Some(Frame::HostRequest { id, method, params })) => {
                        // Caught for a harder reason than the backend's side: this
                        // is the demux thread. A panic here does not lose one
                        // reply, it loses every reply and every event from this
                        // backend for the rest of the session.
                        //
                        // For the same reason a *slow* one is worth saying out
                        // loud: while this call runs, nothing else from this
                        // backend can be delivered, so a reverse-channel handler
                        // that blocks looks exactly like a backend that has
                        // stopped answering — which is the hardest failure in this
                        // whole design to tell apart from a hang.
                        let began = std::time::Instant::now();
                        let result = dispatch_caught(&host_dispatch, &method, params);
                        if began.elapsed() >= SLOW_CALL_NOTICE {
                            eprintln!(
                                "arbor-ipc: host method `{method}` held the reader thread for {}s \
                                 — every reply from this backend waited on it",
                                began.elapsed().as_secs()
                            );
                        }
                        if let Ok(mut w) = writer_for_reader.lock() {
                            let _ = write_frame(&mut *w, &Frame::HostResponse { id, result });
                        }
                    }
                    Ok(Some(_)) => {} // Hello/HostResponse not expected backend → shell
                    Ok(None) | Err(_) => break, // EOF or framing error: child gone
                }
            }
            // Fail any in-flight calls so they don't block forever.
            for (_, tx) in pending.lock().expect("pending poisoned").drain() {
                let _ = tx.send(Err(format!("{program} disconnected")));
            }
            // Signal the shell that the backend is gone (fired once, after the
            // in-flight calls above are unwound).
            on_disconnect();
        });

        Ok((Self { inner }, methods))
    }
}

impl BrokerClient for ChildClient {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
        let value: Value = if params.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&params).map_err(|e| IpcError::Codec(e.to_string()))?
        };

        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.inner
            .pending
            .lock()
            .map_err(|_| IpcError::Transport("pending lock poisoned".into()))?
            .insert(id, tx);

        {
            let mut w = self
                .inner
                .writer
                .lock()
                .map_err(|_| IpcError::Transport("writer lock poisoned".into()))?;
            write_frame(&mut *w, &Frame::Request { id, method: method.to_string(), params: value })
                .map_err(|e| IpcError::Transport(e.to_string()))?;
        }

        // Waited for in slices rather than in one `recv()`, purely so a call that
        // never comes back **says which one it was**.
        //
        // There is no timeout here and there must not be: a ten-minute query is a
        // legitimate thing to be waiting on, and a client that gave up on it would
        // be a worse product than one that waits. What was missing was not a limit,
        // it was a *voice* — a backend that stops answering used to be indis-
        // tinguishable, from every side, from one that is simply busy.
        let mut waited = Duration::ZERO;
        loop {
            match rx.recv_timeout(SLOW_CALL_NOTICE) {
                Ok(Ok(v)) => {
                    return serde_json::to_vec(&v).map_err(|e| IpcError::Codec(e.to_string()))
                }
                Ok(Err(s)) => return Err(IpcError::Backend(s)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    waited += SLOW_CALL_NOTICE;
                    eprintln!(
                        "arbor-ipc: {} has not answered `{method}` (id {id}) after {}s",
                        self.inner.program,
                        waited.as_secs()
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(IpcError::Transport(format!(
                        "{} disconnected",
                        self.inner.program
                    )))
                }
            }
        }
    }
}

/// How long a call may be outstanding before it is mentioned, and then mentioned
/// again. Long enough that an ordinary query never trips it.
const SLOW_CALL_NOTICE: Duration = Duration::from_secs(15);

impl Drop for ChildInner {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;

    /// A reader that answers `WouldBlock` a few times before every real read — a stream some
    /// child made non-blocking, which is what `Read::read_exact` cannot survive.
    struct Stuttering {
        data: Vec<u8>,
        pos: usize,
        /// How many `WouldBlock`s to emit before the next byte.
        stalls: usize,
        left: usize,
    }

    impl Read for Stuttering {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.left > 0 {
                self.left -= 1;
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            self.left = self.stalls;
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            // One byte at a time, so a frame is also reassembled across several reads — the
            // other thing `read_exact` did for us and a hand-rolled loop has to keep doing.
            buf[0] = self.data[self.pos];
            self.pos += 1;
            Ok(1)
        }
    }

    fn framed(frame: &Frame) -> Vec<u8> {
        let mut out = Vec::new();
        write_frame(&mut out, frame).unwrap();
        out
    }

    #[test]
    fn a_stream_that_says_try_again_is_waited_on_rather_than_treated_as_broken() {
        let frame = Frame::Request { id: 7, method: "ping".into(), params: Value::Null };
        let mut r = Stuttering { data: framed(&frame), pos: 0, stalls: 3, left: 3 };

        // The whole point: `read_exact` would have returned `WouldBlock` here and ended the
        // serve loop, reporting a healthy backend as disconnected.
        match read_frame(&mut r).unwrap() {
            Some(Frame::Request { id, method, .. }) => {
                assert_eq!((id, method.as_str()), (7, "ping"));
            }
            other => panic!("expected the request back, got {other:?}"),
        }
        // …and the end of the stream is still an end, not a hang.
        assert!(read_frame(&mut r).unwrap().is_none());
    }

    #[test]
    fn a_stream_that_ends_mid_frame_is_an_error_not_a_clean_close() {
        // Truncated on purpose: a peer that died half-way through a frame has NOT closed
        // cleanly, and treating it as EOF would turn a crash into a silent shutdown.
        let frame = Frame::Request { id: 1, method: "x".into(), params: Value::Null };
        let bytes = framed(&frame);
        let mut r = Stuttering { data: bytes[..bytes.len() - 2].to_vec(), pos: 0, stalls: 0, left: 0 };
        let err = read_frame(&mut r).expect_err("a half-read frame is not a clean close");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn nothing_at_all_is_a_clean_close() {
        let mut r = Stuttering { data: Vec::new(), pos: 0, stalls: 0, left: 0 };
        assert!(read_frame(&mut r).unwrap().is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Condvar;

    use serde_json::json;

    /// A blocking in-memory byte pipe (the test stand-in for one direction of a
    /// process's stdio): `Write` appends + notifies; `Read` blocks until bytes
    /// are available, or returns EOF once [`close`](Pipe::close) is called.
    /// Cloneable — both ends share the same buffer.
    #[derive(Clone)]
    struct Pipe {
        inner: Arc<(Mutex<PipeState>, Condvar)>,
    }

    struct PipeState {
        buf: VecDeque<u8>,
        open: bool,
    }

    impl Pipe {
        fn new() -> Self {
            Self {
                inner: Arc::new((
                    Mutex::new(PipeState { buf: VecDeque::new(), open: true }),
                    Condvar::new(),
                )),
            }
        }

        /// Signal end-of-stream: blocked/future reads see EOF once the buffer drains.
        fn close(&self) {
            let (lock, cv) = &*self.inner;
            lock.lock().unwrap().open = false;
            cv.notify_all();
        }
    }

    impl Write for Pipe {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let (lock, cv) = &*self.inner;
            lock.lock().unwrap().buf.extend(data.iter().copied());
            cv.notify_all();
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Pipe {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let (lock, cv) = &*self.inner;
            let mut st = lock.lock().unwrap();
            loop {
                if !st.buf.is_empty() {
                    let n = out.len().min(st.buf.len());
                    for slot in out.iter_mut().take(n) {
                        *slot = st.buf.pop_front().unwrap();
                    }
                    return Ok(n);
                }
                if !st.open {
                    return Ok(0); // EOF
                }
                st = cv.wait(st).unwrap();
            }
        }
    }

    /// The load-bearing reverse-channel test: a backend handler, **mid-dispatch**,
    /// calls back to the shell and blocks on the reply; the serve loop's reader
    /// must deliver that reply while the handler is parked. Proves no deadlock —
    /// the whole reason `serve_stdio` dispatches off the reader thread.
    #[test]
    fn reentrant_host_call_round_trips_without_deadlock() {
        let be2sh = Pipe::new(); // backend → shell (Hello, HostRequest, Response)
        let sh2be = Pipe::new(); // shell → backend (Request, HostResponse)

        let out: SharedWriter = Arc::new(Mutex::new(be2sh.clone()));
        let host = FrameHostCaller::new(Arc::clone(&out));

        // Handler "trigger" reentrantly asks the shell to "add_one" mid-dispatch.
        let host_for_dispatch: Arc<dyn HostCaller> = Arc::clone(&host) as Arc<dyn HostCaller>;
        let dispatch = move |method: &str, params: Value| -> Result<Value, String> {
            match method {
                "trigger" => {
                    let n = params.as_i64().ok_or("expected int")?;
                    host_for_dispatch.call("add_one", json!(n)) // blocks on the shell
                }
                other => Err(format!("unknown: {other}")),
            }
        };

        let serve_in = sh2be.clone();
        let serve_out = Arc::clone(&out);
        let serve_host = Arc::clone(&host);
        let serve = thread::spawn(move || {
            let _ = serve_stdio(serve_in, serve_out, vec!["trigger".to_string()], serve_host, dispatch, || {});
        });

        // ── Shell side ──
        let mut sh_in = be2sh.clone();
        match read_frame(&mut sh_in).unwrap() {
            Some(Frame::Hello { methods }) => assert_eq!(methods, vec!["trigger".to_string()]),
            other => panic!("expected Hello, got {other:?}"),
        }

        // Fire a request that triggers the reentrant call-back.
        write_frame(&mut sh2be.clone(), &Frame::Request { id: 1, method: "trigger".into(), params: json!(41) }).unwrap();

        // The handler calls back: answer it (the reader is free to receive this
        // even though the worker is parked in `host.call`).
        let hid = match read_frame(&mut sh_in).unwrap() {
            Some(Frame::HostRequest { id, method, params }) => {
                assert_eq!(method, "add_one");
                assert_eq!(params, json!(41));
                id
            }
            other => panic!("expected HostRequest, got {other:?}"),
        };
        write_frame(&mut sh2be.clone(), &Frame::HostResponse { id: hid, result: Ok(json!(42)) }).unwrap();

        // The handler resumes with 42 and replies to the original request.
        match read_frame(&mut sh_in).unwrap() {
            Some(Frame::Response { id, result }) => {
                assert_eq!(id, 1);
                assert_eq!(result, Ok(json!(42)));
            }
            other => panic!("expected Response, got {other:?}"),
        }

        sh2be.close(); // EOF → serve loop exits
        serve.join().unwrap();
    }

    /// The other load-bearing one: a handler that **panics** must still produce a
    /// `Response`.
    ///
    /// Without it the worker thread unwinds past the write, the frame is never
    /// sent, and the caller — blocked on a channel with no timeout — waits
    /// forever. That is not a crash anybody can act on: the window simply stops,
    /// with no error in the frontend and none in the reply, because there is no
    /// reply. It cost a night of "the database will not connect any more".
    ///
    /// The second request proves the loop survives the first: one bad handler must
    /// not take the backend down with it.
    #[test]
    fn a_panicking_handler_still_answers() {
        let be2sh = Pipe::new();
        let sh2be = Pipe::new();

        let out: SharedWriter = Arc::new(Mutex::new(be2sh.clone()));
        let host = FrameHostCaller::new(Arc::clone(&out));

        let dispatch = |method: &str, _params: Value| -> Result<Value, String> {
            match method {
                "boom" => panic!("index out of bounds: the len is 3 but the index is 7"),
                "fine" => Ok(json!("still here")),
                other => Err(format!("unknown: {other}")),
            }
        };

        let serve_in = sh2be.clone();
        let serve_out = Arc::clone(&out);
        let serve_host = Arc::clone(&host);
        let serve = thread::spawn(move || {
            let _ = serve_stdio(serve_in, serve_out, vec!["boom".to_string()], serve_host, dispatch, || {});
        });

        let mut sh_in = be2sh.clone();
        assert!(matches!(read_frame(&mut sh_in).unwrap(), Some(Frame::Hello { .. })));

        write_frame(&mut sh2be.clone(), &Frame::Request { id: 1, method: "boom".into(), params: Value::Null }).unwrap();
        match read_frame(&mut sh_in).unwrap() {
            Some(Frame::Response { id, result }) => {
                assert_eq!(id, 1);
                let message = result.unwrap_err();
                // The caller is told which method, and gets the panic's own text —
                // the only description of the fault that exists anywhere.
                assert!(message.contains("boom"), "{message}");
                assert!(message.contains("index out of bounds"), "{message}");
            }
            other => panic!("expected Response, got {other:?}"),
        }

        write_frame(&mut sh2be.clone(), &Frame::Request { id: 2, method: "fine".into(), params: Value::Null }).unwrap();
        match read_frame(&mut sh_in).unwrap() {
            Some(Frame::Response { id, result }) => {
                assert_eq!(id, 2);
                assert_eq!(result, Ok(json!("still here")));
            }
            other => panic!("expected Response, got {other:?}"),
        }

        sh2be.close();
        serve.join().unwrap();
    }

    /// A burst larger than the worker cap is still answered in full.
    ///
    /// The pool is what stops a stampede from becoming one OS thread per request; this is the
    /// proof that bounding it costs nothing a caller can observe. 200 requests against a cap of
    /// 64 means most of them wait for a worker, and every one of them must still come back —
    /// with its own answer, exactly once.
    #[test]
    fn a_burst_beyond_the_worker_cap_is_answered_in_full() {
        let be2sh = Pipe::new();
        let sh2be = Pipe::new();

        let out: SharedWriter = Arc::new(Mutex::new(be2sh.clone()));
        let host = FrameHostCaller::new(Arc::clone(&out));

        let dispatch = |_method: &str, params: Value| -> Result<Value, String> { Ok(params) };

        let serve_in = sh2be.clone();
        let serve_out = Arc::clone(&out);
        let serve_host = Arc::clone(&host);
        let serve = thread::spawn(move || {
            let _ = serve_stdio(serve_in, serve_out, vec!["echo".to_string()], serve_host, dispatch, || {});
        });

        let mut sh_in = be2sh.clone();
        assert!(matches!(read_frame(&mut sh_in).unwrap(), Some(Frame::Hello { .. })));

        const BURST: u64 = 200;
        for id in 1..=BURST {
            write_frame(
                &mut sh2be.clone(),
                &Frame::Request { id, method: "echo".into(), params: json!(id) },
            )
            .unwrap();
        }

        // Answers may interleave — the pool is concurrent — so they are compared as a set.
        let mut answered = std::collections::HashSet::new();
        for _ in 0..BURST {
            match read_frame(&mut sh_in).unwrap() {
                Some(Frame::Response { id, result }) => {
                    assert_eq!(result, Ok(json!(id)), "request {id} got another request's answer");
                    assert!(answered.insert(id), "request {id} answered twice");
                }
                other => panic!("expected Response, got {other:?}"),
            }
        }
        assert_eq!(answered.len() as u64, BURST);

        sh2be.close();
        serve.join().unwrap();
    }

    /// Handlers still run at the same time.
    ///
    /// The pool reuses threads, and one that reused a single thread would turn every backend into
    /// a queue — a five-minute build would stop the editor answering anything at all. Two handlers
    /// that can only finish together prove it does not: neither returns until both have arrived.
    #[test]
    fn handlers_run_concurrently() {
        let be2sh = Pipe::new();
        let sh2be = Pipe::new();

        let out: SharedWriter = Arc::new(Mutex::new(be2sh.clone()));
        let host = FrameHostCaller::new(Arc::clone(&out));

        let gate = Arc::new((Mutex::new(0u32), Condvar::new()));
        let gate_for_dispatch = Arc::clone(&gate);
        let dispatch = move |_method: &str, _params: Value| -> Result<Value, String> {
            let (lock, cv) = &*gate_for_dispatch;
            let mut arrived = lock.lock().unwrap();
            *arrived += 1;
            cv.notify_all();
            while *arrived < 2 {
                let (next, timeout) = cv.wait_timeout(arrived, Duration::from_secs(10)).unwrap();
                arrived = next;
                // Reached only if the two were run one after the other, which is the failure.
                if timeout.timed_out() {
                    return Err("handlers were serialised".to_string());
                }
            }
            Ok(json!("both"))
        };

        let serve_in = sh2be.clone();
        let serve_out = Arc::clone(&out);
        let serve_host = Arc::clone(&host);
        let serve = thread::spawn(move || {
            let _ = serve_stdio(serve_in, serve_out, vec!["wait".to_string()], serve_host, dispatch, || {});
        });

        let mut sh_in = be2sh.clone();
        assert!(matches!(read_frame(&mut sh_in).unwrap(), Some(Frame::Hello { .. })));

        for id in 1..=2u64 {
            write_frame(&mut sh2be.clone(), &Frame::Request { id, method: "wait".into(), params: Value::Null }).unwrap();
        }
        for _ in 0..2 {
            match read_frame(&mut sh_in).unwrap() {
                Some(Frame::Response { result, .. }) => assert_eq!(result, Ok(json!("both"))),
                other => panic!("expected Response, got {other:?}"),
            }
        }

        sh2be.close();
        serve.join().unwrap();
    }
}
