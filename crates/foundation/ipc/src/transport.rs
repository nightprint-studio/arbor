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
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Read one frame, or `None` at clean EOF (peer closed the stream).
fn read_frame<R: Read + ?Sized>(r: &mut R) -> io::Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
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

/// Run the backend serve loop: announce `methods` via `Hello`, run `on_ready`
/// (post-Hello startup work — see below), then read frames
/// from `input` (the backend's stdin). Each `Request` is dispatched on its **own
/// worker thread** (so the reader stays free to receive `HostResponse`s while a
/// handler is mid-`HostRequest` — the reverse-channel reentrancy requirement);
/// the worker writes its `Response` on `out` when done. `HostResponse`s are
/// routed back to the matching blocked [`FrameHostCaller::call`] via `host`'s
/// shared pending map.
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
    let mut reader = input;
    while let Some(frame) = read_frame(&mut reader)? {
        match frame {
            Frame::Request { id, method, params } => {
                // Dispatch off the reader thread so a handler that calls back to
                // the shell (and blocks) doesn't stall the reader that must
                // deliver its `HostResponse`.
                let out = Arc::clone(&out);
                let dispatch = Arc::clone(&dispatch);
                thread::spawn(move || {
                    let result = dispatch_caught(&*dispatch, &method, params);
                    if let Ok(mut w) = out.lock() {
                        let _ = write_frame(&mut *w, &Frame::Response { id, result });
                    }
                });
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
}
