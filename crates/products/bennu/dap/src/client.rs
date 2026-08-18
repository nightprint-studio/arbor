//! The transport: one debug-adapter child process, spoken to over its stdio.
//!
//! Owns the process, the request/response correlation, and the adapter's own traffic back at us.
//! Knows nothing about what the messages mean — [`crate::session`] decides that.
//!
//! ## Threads
//!
//! Three, per adapter:
//!
//! * the **reader**, blocked on the child's stdout, which is the only thing that ever parses an
//!   incoming frame;
//! * the **stderr drain**, which keeps the tail of the adapter's log. Not optional: when an adapter
//!   refuses to start, its stderr is the only place the reason is written down — `gdb` older than 14
//!   prints a usage error there and exits — and a client that discards it can only report "it didn't
//!   work";
//! * whichever caller thread is making a request, which blocks on a channel until the reader hands
//!   it a response.
//!
//! An adapter→client **request** is dispatched on a short-lived worker thread rather than inline on
//! the reader. Same landmine as `docs/reverse-channel.md` documents for Bennu's own IPC seam, and the
//! same one the LSP client avoids: a handler that answers by making a request *back* would be waiting
//! for a response only the reader can deliver, and the reader is inside the handler. Events stay
//! inline — they need no reply — which is why [`AdapterHandler`]'s event method must not block.
//!
//! ## Failure model
//!
//! A dead adapter is not something to recover from mid-request: once the process is gone every pending
//! caller must be released rather than left on a channel nobody will write. So the reader, on end of
//! stream, marks the client dead and drains `pending` — every waiting caller is failed immediately
//! instead of waiting out its timeout.
//!
//! ## `success: false` is not an error here
//!
//! A refused request comes back as a [`Response`] with `success: false`, and this module hands it to
//! the caller as `Ok`. Which is right: "there is no variable called `foo`" is an answer, and the
//! caller is the only thing that knows whether that is a failure or a fact. Only the transport dying
//! and the timeout expiring are [`DapError`]s.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::Duration;

use arbor_process_ext::NoWindowExt;
use serde::Serialize;

use crate::protocol::{Incoming, Message, Outgoing, Response, Seq};

/// How many lines of the adapter's stderr to keep.
///
/// Enough to hold a Rust panic with its backtrace header, and enough for `gdb`'s usage text — the two
/// shapes of "it would not start" that carry the reason.
const STDERR_TAIL: usize = 200;

/// How long to wait for a `disconnect` to be acknowledged before killing the process.
///
/// An adapter mid-`launch` of a large binary will not answer promptly, and blocking a window close on
/// it is worse than killing it.
const DISCONNECT_GRACE: Duration = Duration::from_millis(1500);

/// What went wrong with a request — the transport, never the debuggee.
#[derive(Debug, Clone)]
pub enum DapError {
    /// The adapter is not running: never started, crashed, or was stopped.
    NotRunning,
    /// The pipe broke, or a frame was malformed.
    Transport(String),
    /// The adapter did not answer in time.
    Timeout {
        command: String,
        after: Duration,
    },
    /// The adapter could not be started at all. Carries its stderr, which is where the reason is.
    Spawn(String),
}

impl std::fmt::Display for DapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DapError::NotRunning => write!(f, "the debug adapter is not running"),
            DapError::Transport(e) => write!(f, "the connection to the debug adapter failed: {e}"),
            DapError::Timeout { command, after } => {
                write!(f, "the debug adapter did not answer `{command}` within {after:?}")
            }
            DapError::Spawn(e) => write!(f, "the debug adapter would not start: {e}"),
        }
    }
}

impl std::error::Error for DapError {}

/// What the host does with the adapter's unsolicited traffic.
///
/// **Nothing here may block.** Events are dispatched inline on the reader thread, so a handler that
/// waits on anything stops the only thread that can deliver what it is waiting for. Anything slow
/// belongs on a thread of the implementor's own.
pub trait AdapterHandler: Send + Sync {
    /// Something happened in the debuggee: `stopped`, `output`, `terminated`, …
    fn on_event(&self, event: crate::protocol::Event);

    /// The adapter is asking us for something and is **blocked** until this returns.
    ///
    /// Returns the response body, or an error string to refuse with. The default refuses everything,
    /// which is the honest answer for a client that declared no client-side capabilities: an adapter
    /// only sends `runInTerminal` if we said we support it.
    fn on_request(&self, command: &str, _arguments: Option<serde_json::Value>) -> Result<Option<serde_json::Value>, String> {
        Err(format!("Bennu does not implement the `{command}` reverse request"))
    }

    /// The adapter's process ended. `reason` is for a user.
    fn on_exit(&self, reason: &str);
}

/// A request that has been sent and not yet answered — see [`DapClient::request_async`].
///
/// Holds the channel the reader will write to, so dropping one without collecting it simply discards
/// the answer, which is the right behaviour for a caller that stopped caring.
pub struct Pending {
    seq: i64,
    command: String,
    rx: mpsc::Receiver<Response>,
}

/// One adapter process.
pub struct DapClient {
    /// For the error messages: which adapter this is.
    label: String,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    /// Keyed on **our** seq — see [`crate::protocol`] for why not the adapter's.
    pending: Mutex<HashMap<i64, mpsc::Sender<Response>>>,
    seq: Seq,
    alive: AtomicBool,
    stderr: Arc<Mutex<VecDeque<String>>>,
    handler: Arc<dyn AdapterHandler>,
}

impl DapClient {
    /// Start `exe args…` and begin reading it.
    ///
    /// `cwd` is the debuggee's project root, which matters: an adapter resolves relative source paths
    /// against it, and `codelldb` reads a `.lldbinit` from it.
    pub fn spawn(
        label: &str,
        exe: &str,
        args: &[String],
        cwd: &str,
        handler: Arc<dyn AdapterHandler>,
    ) -> Result<Arc<DapClient>, DapError> {
        let mut command = Command::new(exe);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // A GUI process spawning a console one flashes a window on Windows, and an adapter is
        // spawned every time a session starts.
        command.no_window();

        let mut child = command.spawn().map_err(|e| DapError::Spawn(e.to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| DapError::Spawn("no stdout".into()))?;
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();

        let client = Arc::new(DapClient {
            label: label.to_string(),
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            seq: Seq::default(),
            alive: AtomicBool::new(true),
            stderr: Arc::new(Mutex::new(VecDeque::new())),
            handler,
        });

        // Weak, so the reader thread cannot keep a dead session's process alive: when the last strong
        // reference goes, `Drop` kills the child and the reader's next read ends the loop.
        let weak = Arc::downgrade(&client);
        let name = format!("dap-reader-{label}");
        std::thread::Builder::new()
            .name(name)
            .spawn(move || read_loop(weak, BufReader::new(stdout)))
            .map_err(|e| DapError::Spawn(e.to_string()))?;

        if let Some(stderr) = stderr {
            let tail = Arc::clone(&client.stderr);
            let name = format!("dap-stderr-{label}");
            let _ = std::thread::Builder::new().name(name).spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut guard = tail.lock().unwrap_or_else(|p| p.into_inner());
                    if guard.len() == STDERR_TAIL {
                        guard.pop_front();
                    }
                    guard.push_back(line);
                }
            });
        }

        Ok(client)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// The tail of the adapter's stderr, newest last. The only place a refusal to start says why.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect()
    }

    /// Send a request and wait for its answer.
    ///
    /// A refused request is `Ok(response)` with `success: false` — see the module docs.
    pub fn request<A: Serialize>(
        &self,
        command: &str,
        arguments: Option<A>,
        timeout: Duration,
    ) -> Result<Response, DapError> {
        let pending = self.request_async(command, arguments)?;
        self.await_response(pending, timeout)
    }

    /// Send a request **without** waiting, and get a token to collect the answer with later.
    ///
    /// Needed for exactly one thing, and it is not an optimisation: several adapters hold the `launch`
    /// response until `configurationDone` arrives, so a client that waits for it before sending that
    /// deadlocks — each side waiting for the other. The spec warns about it, and this is what lets the
    /// session send `launch`, do the configuration sequence, and then collect the answer.
    pub fn request_async<A: Serialize>(
        &self,
        command: &str,
        arguments: Option<A>,
    ) -> Result<Pending, DapError> {
        if !self.is_alive() {
            return Err(DapError::NotRunning);
        }
        let arguments = match arguments {
            Some(a) => Some(
                serde_json::to_value(a)
                    .map_err(|e| DapError::Transport(format!("could not encode arguments: {e}")))?,
            ),
            None => None,
        };
        let seq = self.seq.next();
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).insert(seq, tx);

        let out = Outgoing::Request { seq, command: command.to_string(), arguments };
        if let Err(e) = self.send(&out) {
            self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&seq);
            return Err(e);
        }
        Ok(Pending { seq, command: command.to_string(), rx })
    }

    /// Collect the answer to a request sent with [`Self::request_async`].
    pub fn await_response(
        &self,
        pending: Pending,
        timeout: Duration,
    ) -> Result<Response, DapError> {
        match pending.rx.recv_timeout(timeout) {
            Ok(response) => Ok(response),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Forget the waiter: a late answer must not be delivered to a caller that has moved
                // on, and leaving the entry would leak one per timed-out request.
                self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&pending.seq);
                Err(DapError::Timeout { command: pending.command, after: timeout })
            }
            // The sender was dropped, which is `mark_dead` releasing us.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DapError::NotRunning),
        }
    }

    /// Answer one of the adapter's own requests.
    fn respond(
        &self,
        request_seq: i64,
        command: &str,
        result: Result<Option<serde_json::Value>, String>,
    ) {
        let seq = self.seq.next();
        let out = match result {
            Ok(body) => Outgoing::Response {
                seq,
                request_seq,
                success: true,
                command: command.to_string(),
                message: None,
                body,
            },
            Err(message) => Outgoing::Response {
                seq,
                request_seq,
                success: false,
                command: command.to_string(),
                message: Some(message),
                body: None,
            },
        };
        let _ = self.send(&out);
    }

    fn send(&self, out: &Outgoing) -> Result<(), DapError> {
        let body = serde_json::to_vec(out)
            .map_err(|e| DapError::Transport(format!("could not encode a message: {e}")))?;
        // The guard is scoped tightly so it is released before `mark_dead` runs below — that path
        // calls back into the handler, and holding the write lock across a callback is how a
        // transport deadlocks.
        let written = {
            let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
            match guard.as_mut() {
                Some(stdin) => bennu_framed::write_message(stdin, &body),
                None => return Err(DapError::NotRunning),
            }
        };
        match written {
            Ok(()) => Ok(()),
            Err(e) => {
                self.mark_dead("the connection to the debug adapter was lost");
                Err(DapError::Transport(e.to_string()))
            }
        }
    }

    /// Complete a pending request, or note that nobody was waiting.
    fn deliver(&self, response: Response) {
        let tx = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&response.request_seq);
        if let Some(tx) = tx {
            let _ = tx.send(response); // the caller may have timed out and left
        }
    }

    /// Mark the session dead and release every caller blocked on a response.
    ///
    /// Draining `pending` is the load-bearing half: without it each waiting caller sits out its whole
    /// timeout for an answer that provably cannot arrive, and a crashed adapter turns into a panel
    /// that hangs for seconds per button.
    fn mark_dead(&self, reason: &str) {
        if !self.alive.swap(false, Ordering::SeqCst) {
            return; // already reported
        }
        let waiters: Vec<_> = {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            pending.drain().map(|(_, tx)| tx).collect()
        };
        // Dropping each sender is what wakes its caller with `Disconnected`.
        drop(waiters);
        self.handler.on_exit(reason);
    }

    /// Ask the adapter to end the session, then make sure it has.
    ///
    /// `terminate_first` sends `terminate` before `disconnect` — the polite stop, which lets the
    /// debuggee run its exit path. Only for an adapter that said it supports it; sending it to one
    /// that did not is an error on several.
    pub fn shutdown(&self, terminate_first: bool) {
        if self.is_alive() {
            if terminate_first {
                let _ = self.request(
                    "terminate",
                    Some(serde_json::json!({ "restart": false })),
                    DISCONNECT_GRACE,
                );
            }
            let _ = self.request(
                "disconnect",
                Some(serde_json::json!({ "restart": false, "terminateDebuggee": true })),
                DISCONNECT_GRACE,
            );
        }
        // Closing stdin is the second signal: an adapter blocked on reading its input exits on end of
        // stream even if it ignored `disconnect`.
        self.stdin.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.mark_dead("the debug session ended");
        self.kill();
    }

    /// Kill the process and reap it, so it cannot outlive the backend.
    fn kill(&self) {
        let mut guard = self.child.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        // Never leave an orphan: an adapter that outlives its session holds the debuggee suspended,
        // and a suspended process with no debugger attached is one nothing can release.
        self.stdin.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.kill();
    }
}

/// The reader thread: parse frames until the stream ends, dispatching each one.
fn read_loop<R: BufRead>(weak: Weak<DapClient>, mut reader: R) {
    // The peer name reaches the user through a transport error, so it says which process went away.
    let label = weak.upgrade().map(|c| c.label.clone()).unwrap_or_else(|| "adapter".to_string());
    let peer = format!("the {label} debug adapter");
    loop {
        let frame = match bennu_framed::read_message(&mut reader, &peer) {
            Ok(Some(body)) => body,
            Ok(None) => {
                if let Some(client) = weak.upgrade() {
                    client.mark_dead("the debug adapter exited");
                }
                return;
            }
            Err(e) => {
                if let Some(client) = weak.upgrade() {
                    client.mark_dead(&format!("the debug adapter's output could not be read: {e}"));
                }
                return;
            }
        };
        let Some(client) = weak.upgrade() else { return };

        let Ok(incoming) = serde_json::from_slice::<Incoming>(&frame) else {
            // One unparseable message is not a desync — the framing held, so the next frame starts
            // where it should. Dropping it beats tearing down a working session.
            continue;
        };
        match incoming.classify() {
            Some(Message::Response(response)) => client.deliver(response),
            // Inline, and `AdapterHandler` documents that it must not block: this is the only thread
            // that can deliver anything.
            Some(Message::Event(event)) => client.handler.on_event(event),
            Some(Message::Request(request)) => {
                // On a worker, never inline. A handler that answers by asking the adapter something
                // would be waiting for a response only this thread can deliver — see the module docs.
                let client = Arc::clone(&client);
                let _ = std::thread::Builder::new().name("dap-reverse".to_string()).spawn(
                    move || {
                        let result =
                            client.handler.on_request(&request.command, request.arguments);
                        client.respond(request.seq, &request.command, result);
                    },
                );
            }
            // Not one of the three. Dropping it is right: there is nothing it could be dispatched to,
            // and the framing is still intact.
            None => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Event;

    /// A handler that records what it was given, so a test can assert on the dispatch.
    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<String>>,
        exits: Mutex<Vec<String>>,
    }

    impl AdapterHandler for Recorder {
        fn on_event(&self, event: Event) {
            self.events.lock().unwrap().push(event.event);
        }
        fn on_exit(&self, reason: &str) {
            self.exits.lock().unwrap().push(reason.to_string());
        }
    }

    /// A client with no process behind it, for the parts that do not need one.
    fn detached(handler: Arc<dyn AdapterHandler>) -> Arc<DapClient> {
        Arc::new(DapClient {
            label: "test".into(),
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            seq: Seq::default(),
            alive: AtomicBool::new(true),
            stderr: Arc::new(Mutex::new(VecDeque::new())),
            handler,
        })
    }

    #[test]
    fn a_request_on_a_dead_client_fails_at_once() {
        let client = detached(Arc::new(Recorder::default()));
        client.alive.store(false, Ordering::SeqCst);
        let err = client
            .request("threads", None::<()>, Duration::from_millis(10))
            .expect_err("must not wait");
        assert!(matches!(err, DapError::NotRunning), "{err:?}");
    }

    /// The load-bearing half of the failure model: a caller blocked on an answer must be released the
    /// moment the adapter dies, not left to time out.
    #[test]
    fn marking_dead_releases_every_waiting_caller() {
        let recorder = Arc::new(Recorder::default());
        let client = detached(recorder.clone());

        let (tx, rx) = mpsc::channel();
        client.pending.lock().unwrap().insert(7, tx);
        client.mark_dead("the debug adapter exited");

        assert!(matches!(rx.recv(), Err(mpsc::RecvError)), "the waiter must be woken, not left");
        assert!(!client.is_alive());
        assert_eq!(recorder.exits.lock().unwrap().as_slice(), &["the debug adapter exited"]);
    }

    #[test]
    fn a_second_death_is_not_reported_twice() {
        let recorder = Arc::new(Recorder::default());
        let client = detached(recorder.clone());
        client.mark_dead("first");
        client.mark_dead("second");
        assert_eq!(recorder.exits.lock().unwrap().len(), 1, "one exit per session");
    }

    #[test]
    fn a_response_reaches_the_caller_that_asked() {
        let client = detached(Arc::new(Recorder::default()));
        let (tx, rx) = mpsc::channel();
        client.pending.lock().unwrap().insert(3, tx);

        client.deliver(Response {
            request_seq: 3,
            command: "threads".into(),
            success: true,
            message: None,
            body: None,
        });
        assert_eq!(rx.recv().unwrap().command, "threads");
        // …and the entry is gone, so a duplicate answer does not go anywhere.
        assert!(client.pending.lock().unwrap().is_empty());
    }

    /// A late answer to a request whose caller timed out must be dropped, not delivered.
    #[test]
    fn a_response_nobody_is_waiting_for_is_dropped() {
        let client = detached(Arc::new(Recorder::default()));
        client.deliver(Response {
            request_seq: 99,
            command: "evaluate".into(),
            success: true,
            message: None,
            body: None,
        });
        // Nothing panicked, and nothing was invented to receive it.
        assert!(client.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn the_default_reverse_request_handler_refuses_by_name() {
        struct Bare;
        impl AdapterHandler for Bare {
            fn on_event(&self, _: Event) {}
            fn on_exit(&self, _: &str) {}
        }
        let err = Bare.on_request("runInTerminal", None).expect_err("must refuse");
        assert!(err.contains("runInTerminal"), "{err}");
    }

    #[test]
    fn a_timeout_names_the_command_and_the_wait() {
        let e = DapError::Timeout { command: "stackTrace".into(), after: Duration::from_secs(2) };
        let text = e.to_string();
        assert!(text.contains("stackTrace"), "{text}");
        assert!(text.contains('2'), "{text}");
    }

    #[test]
    fn the_stderr_tail_is_bounded_and_keeps_the_newest() {
        let client = detached(Arc::new(Recorder::default()));
        {
            let mut guard = client.stderr.lock().unwrap();
            for i in 0..(STDERR_TAIL + 10) {
                if guard.len() == STDERR_TAIL {
                    guard.pop_front();
                }
                guard.push_back(format!("line {i}"));
            }
        }
        let tail = client.stderr_tail();
        assert_eq!(tail.len(), STDERR_TAIL);
        // The reason an adapter died is at the END of its log, so that is the half to keep.
        assert_eq!(tail.last().unwrap(), &format!("line {}", STDERR_TAIL + 9));
    }

    #[test]
    fn spawning_something_that_is_not_there_is_a_spawn_error_and_not_a_panic() {
        let started =
            DapClient::spawn("nope", "/definitely/not/an/adapter", &[], ".", Arc::new(Recorder::default()));
        // `DapClient` holds a live child and is deliberately not `Debug`, so the error is matched
        // rather than unwrapped.
        match started {
            Err(DapError::Spawn(_)) => {}
            Err(other) => panic!("expected a spawn failure, got {other:?}"),
            Ok(_) => panic!("that path is not an adapter"),
        }
    }
}
