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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{BrokerClient, Bytes};
use crate::error::IpcError;
use crate::event::EventSink;

/// A shared, type-erased frame writer — the backend's stdout (the event sink and
/// the serve loop both write through it, serialized by the mutex).
pub type SharedWriter = Arc<Mutex<dyn Write + Send>>;

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

/// Run the backend serve loop: announce `methods` via `Hello`, then read
/// `Request`s from stdin and reply with `Response`s on `out` (the backend's
/// stdout). `dispatch` maps `(method, params)` to a result; the event channel
/// runs concurrently through [`FrameEventSink`] sharing the same `out`.
///
/// Returns when the peer closes stdin (the shell exited).
pub fn serve_stdio<F>(out: SharedWriter, methods: Vec<String>, dispatch: F) -> io::Result<()>
where
    F: Fn(&str, Value) -> Result<Value, String>,
{
    {
        let mut w = out.lock().expect("frame writer poisoned");
        write_frame(&mut *w, &Frame::Hello { methods })?;
    }

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    while let Some(frame) = read_frame(&mut reader)? {
        if let Frame::Request { id, method, params } = frame {
            let result = dispatch(&method, params);
            let mut w = out.lock().expect("frame writer poisoned");
            write_frame(&mut *w, &Frame::Response { id, result })?;
        }
        // Hello/Response/Event are not expected shell → backend; ignore.
    }
    Ok(())
}

// ── Shell side ──────────────────────────────────────────────────────────────

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>;

/// A [`BrokerClient`] backed by a spawned child process, framed over its stdio.
///
/// A background thread reads the child's stdout and demuxes: `Response` frames
/// wake the matching blocked [`call`](BrokerClient::call); `Event` frames go to
/// the `on_event` callback (the shell re-emits them to the FE). The child is
/// killed when this client drops.
pub struct ChildClient {
    inner: Arc<ChildInner>,
}

struct ChildInner {
    /// Child stdin — where requests are written (serialized by the mutex).
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Pending,
    next_id: AtomicU64,
    /// Kept so the child is killed on drop (closing the pipes alone leaves it).
    child: Mutex<Option<Child>>,
}

impl ChildClient {
    /// Spawn `cmd` (its stdin/stdout are overridden to pipes; stderr is left as
    /// configured), read the backend's `Hello`, and start the reader thread.
    /// Returns the client plus the method names the backend advertised.
    ///
    /// `on_event` is invoked for every push event the backend emits.
    pub fn spawn<E>(mut cmd: Command, on_event: E) -> io::Result<(Self, Vec<String>)>
    where
        E: Fn(String, Value) + Send + 'static,
    {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "child stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "child stdout missing"))?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let inner = Arc::new(ChildInner {
            writer: Mutex::new(Box::new(stdin)),
            pending: Arc::clone(&pending),
            next_id: AtomicU64::new(1),
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

        // Demux replies + events until the child closes its stdout.
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
                    Ok(Some(_)) => {} // Hello/Request not expected backend → shell
                    Ok(None) | Err(_) => break, // EOF or framing error: child gone
                }
            }
            // Fail any in-flight calls so they don't block forever.
            for (_, tx) in pending.lock().expect("pending poisoned").drain() {
                let _ = tx.send(Err("corvus-be disconnected".to_string()));
            }
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

        match rx.recv() {
            Ok(Ok(v)) => serde_json::to_vec(&v).map_err(|e| IpcError::Codec(e.to_string())),
            Ok(Err(s)) => Err(IpcError::Backend(s)),
            Err(_) => Err(IpcError::Transport("corvus-be disconnected".into())),
        }
    }
}

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
