//! The transport: one language-server child process, spoken to over its stdio.
//!
//! Owns the process, the request/response correlation, and the server's own traffic back
//! at us. Knows nothing about what the messages mean — [`crate::session`] decides that.
//!
//! ## Threads
//!
//! Three, per server:
//!
//! * the **reader**, blocked on the child's stdout, which is the only thing that ever
//!   parses an incoming frame;
//! * the **stderr drain**, which keeps the tail of the server's log. Not optional: when a
//!   server refuses to start, its stderr is the only place the reason is written down, and
//!   a client that discards it can only report "it didn't work";
//! * whichever caller thread is making a request, which blocks on a channel until the
//!   reader hands it a response.
//!
//! A server→client **request** is dispatched on a short-lived worker thread rather than
//! inline on the reader. This is the same landmine documented in `docs/reverse-channel.md`
//! for Bennu's own IPC seam: a handler that answers a server request by making a request
//! *back* would be waiting for a response only the reader can deliver, and the reader is
//! inside the handler. Notifications stay inline — they cannot need a reply — which is why
//! [`ServerHandler`]'s notification methods must not block.
//!
//! ## Failure model
//!
//! A dead server is not an error condition to recover from mid-request: once the process
//! is gone every pending caller must be released rather than left on a channel that will
//! never be written. So the reader, on end of stream, marks the client dead and drains
//! `pending` — every waiting caller gets [`LspError::NotRunning`] immediately instead of
//! waiting out its timeout.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::Duration;

use arbor_process_ext::NoWindowExt;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::jsonrpc::{
    self, Incoming, Message, OutgoingNotification, OutgoingRequest, OutgoingResponse, RequestId,
    ResponseError, ERR_METHOD_NOT_FOUND, JSONRPC_VERSION,
};
use crate::types::{
    method, ApplyWorkspaceEditParams, ProgressParams, PublishDiagnosticsParams, ShowMessageParams,
    WorkspaceFolder,
};

/// How many lines of the server's stderr to keep.
///
/// Enough to hold a Rust panic with its backtrace header, which is the shape of the most
/// informative failure a server produces.
const STDERR_TAIL: usize = 200;

/// How long to wait for the graceful `shutdown` handshake before killing the process. A
/// server that is mid-`cargo check` will not answer promptly, and blocking a window close
/// on it is worse than killing it.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);

/// What went wrong with a request.
#[derive(Debug, Clone)]
pub enum LspError {
    /// The server is not running (never started, crashed, or was stopped).
    NotRunning,
    /// The pipe broke, or a frame was malformed.
    Transport(String),
    /// The server answered with an error.
    Server(ResponseError),
    /// The server did not answer in time. Carries the method, because "which request
    /// hung" is the whole diagnostic.
    Timeout(&'static str),
    /// The answer did not match the shape we expected.
    Decode(String),
    /// The server never advertised this capability, so it was not asked.
    Unsupported(&'static str),
}

impl std::fmt::Display for LspError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LspError::NotRunning => write!(f, "the language server is not running"),
            LspError::Transport(e) => write!(f, "language server transport: {e}"),
            LspError::Server(e) => write!(f, "language server error: {e}"),
            LspError::Timeout(m) => write!(f, "the language server did not answer {m} in time"),
            LspError::Decode(e) => write!(f, "unexpected language server answer: {e}"),
            LspError::Unsupported(m) => write!(f, "the language server does not support {m}"),
        }
    }
}

impl std::error::Error for LspError {}

impl LspError {
    /// Whether this is a "ask again later" failure rather than a broken feature.
    ///
    /// The distinction decides what the user sees: a cancelled request (the buffer changed
    /// under it) or a timeout during the initial index should surface as *no answer yet*,
    /// while a genuine error deserves to be reported. Showing "go-to failed" every time a
    /// keystroke races a request teaches the user the feature is unreliable when it is
    /// working exactly as designed.
    pub fn is_transient(&self) -> bool {
        match self {
            LspError::Server(e) => e.is_transient(),
            LspError::Timeout(_) | LspError::NotRunning => true,
            _ => false,
        }
    }
}

/// Callbacks for everything the server sends unprompted.
///
/// **Notification methods must not block.** They run on the reader thread, so a slow one
/// stalls every response the client is waiting for — including the one whose handler is
/// blocking. Lock, push, return.
pub trait ServerHandler: Send + Sync {
    /// `textDocument/publishDiagnostics` — the server's diagnostics are pushed, not
    /// polled, and they arrive seconds after an edit (rust-analyzer runs `cargo check`),
    /// so this is the only way to get them.
    fn on_diagnostics(&self, _params: PublishDiagnosticsParams) {}

    /// `$/progress` — how a server reports "indexing", which is the difference between a
    /// project that looks broken for its first ten seconds and one that says what it is
    /// doing.
    fn on_progress(&self, _params: ProgressParams) {}

    /// `window/showMessage` (`is_log` false) and `window/logMessage` (true).
    fn on_message(&self, _params: ShowMessageParams, _is_log: bool) {}

    /// `workspace/applyEdit` — the server wants to edit the workspace itself (how some
    /// code actions deliver their result). Return whether it was applied; the answer is
    /// part of the protocol and a server may branch on it.
    ///
    /// Runs on a worker thread, so this one *may* block.
    fn on_apply_edit(&self, _params: ApplyWorkspaceEditParams) -> bool {
        false
    }

    /// The process ended, for whatever reason. `reason` is a short human string.
    fn on_exit(&self, _reason: &str) {}
}

/// A running language server.
pub struct LspClient {
    /// `None` once the process has been reaped.
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicI64,
    /// Callers waiting for a response, keyed by request id.
    pending: Mutex<HashMap<i64, mpsc::Sender<Result<serde_json::Value, ResponseError>>>>,
    alive: AtomicBool,
    stderr_tail: Mutex<VecDeque<String>>,
    /// The folders we told the server about, so `workspace/workspaceFolders` can be
    /// answered from the transport without troubling the session.
    folders: Vec<WorkspaceFolder>,
    handler: Arc<dyn ServerHandler>,
    /// For log lines, so two servers in one process are distinguishable.
    tag: String,
}

impl LspClient {
    /// Spawn `command` and start serving it.
    ///
    /// `Err` when the process cannot be started at all — which is the common case worth
    /// getting right (the binary is not installed) and is reported with the command that
    /// was tried, since "No such file or directory" alone names nothing.
    pub fn spawn(
        tag: &str,
        command: &str,
        args: &[String],
        cwd: &std::path::Path,
        env: &[(String, String)],
        folders: Vec<WorkspaceFolder>,
        handler: Arc<dyn ServerHandler>,
    ) -> Result<Arc<Self>, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        // A language server is long-lived: on Windows a bare spawn would leave a console
        // window open for the whole session.
        cmd.no_window();

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start `{command}`: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin on the language server process")?;
        let stdout = child.stdout.take().ok_or("no stdout on the language server process")?;
        let stderr = child.stderr.take().ok_or("no stderr on the language server process")?;

        let client = Arc::new(Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            stderr_tail: Mutex::new(VecDeque::with_capacity(STDERR_TAIL)),
            folders,
            handler,
            tag: tag.to_string(),
        });

        // Both threads hold a WEAK reference. An `Arc` would keep the client alive for as
        // long as the reader is blocked on a read, which is forever — so `Drop` (which is
        // what kills the process and ends the read) could never run.
        let weak = Arc::downgrade(&client);
        let name = format!("lsp-read-{tag}");
        std::thread::Builder::new()
            .name(name)
            .spawn(move || read_loop(weak, BufReader::new(stdout)))
            .map_err(|e| format!("could not start the language server reader thread: {e}"))?;

        let weak = Arc::downgrade(&client);
        let tag_owned = tag.to_string();
        std::thread::Builder::new()
            .name(format!("lsp-err-{tag}"))
            .spawn(move || stderr_loop(weak, BufReader::new(stderr), &tag_owned))
            .map_err(|e| format!("could not start the language server stderr thread: {e}"))?;

        Ok(client)
    }

    /// Whether the process is still up.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// The tail of the server's stderr, oldest first.
    pub fn log_tail(&self) -> Vec<String> {
        self.stderr_tail.lock().unwrap_or_else(|p| p.into_inner()).iter().cloned().collect()
    }

    /// The stderr tail, giving the drain thread up to `grace` to catch up first.
    ///
    /// A server that refuses to start writes its reason on stderr and exits, and those are two
    /// different threads racing: the reader sees EOF and releases the caller while the drain may
    /// not have read the line yet. Snapshotting immediately therefore tends to report the *one*
    /// case where the explanation exists as "the language server is not running".
    ///
    /// Bounded and only on the failure path, so a healthy start pays nothing.
    pub fn log_tail_settled(&self, grace: Duration) -> Vec<String> {
        let deadline = std::time::Instant::now() + grace;
        loop {
            let tail = self.log_tail();
            if !tail.is_empty() || std::time::Instant::now() >= deadline {
                return tail;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// The line of the server's stderr most likely to explain a failure.
    ///
    /// Prefers a line that names an error; falls back to the last non-empty one, which for a
    /// process that died on startup is usually the reason it died.
    pub fn failure_line(tail: &[String]) -> Option<String> {
        const MARKERS: [&str; 6] =
            ["error", "Error", "unknown", "Unknown", "not found", "No such file"];
        tail.iter()
            .rev()
            .find(|l| MARKERS.iter().any(|m| l.contains(m)))
            .or_else(|| tail.iter().rev().find(|l| !l.trim().is_empty()))
            .map(|l| l.trim().to_string())
    }

    /// Send a request and block until the answer arrives, `timeout` elapses, or the server
    /// dies.
    pub fn request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &'static str,
        params: P,
        timeout: Duration,
    ) -> Result<R, LspError> {
        let raw = self.request_raw(method, params, timeout)?;
        serde_json::from_value(raw).map_err(|e| LspError::Decode(format!("{method}: {e}")))
    }

    /// [`request`](Self::request) without the final deserialization — for the callers that
    /// need the untouched JSON (a diagnostic echoed back into a `codeAction`, an opaque
    /// `data` blob).
    pub fn request_raw<P: Serialize>(
        &self,
        method: &'static str,
        params: P,
        timeout: Duration,
    ) -> Result<serde_json::Value, LspError> {
        if !self.is_alive() {
            return Err(LspError::NotRunning);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        // Registered BEFORE the write: a fast server can answer while we are still on this
        // line, and a response with no pending entry is a dropped answer.
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);

        let body = serde_json::to_vec(&OutgoingRequest {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        })
        .map_err(|e| LspError::Transport(format!("could not encode {method}: {e}")))?;

        if let Err(e) = self.write_frame(&body) {
            self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            return Err(e);
        }

        match rx.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(LspError::Server(err)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Drop the waiter and tell the server to stop working on it — otherwise a
                // slow server accumulates abandoned requests and keeps computing answers
                // nobody will read.
                self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
                let _ = self.notify(
                    method::CANCEL_REQUEST,
                    serde_json::json!({ "id": id }),
                );
                Err(LspError::Timeout(method))
            }
            // The sender was dropped: the reader loop tore down `pending` because the
            // process died.
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(LspError::NotRunning),
        }
    }

    /// Send a notification. Fire-and-forget by definition — there is no answer to wait for
    /// and no error the server can report.
    pub fn notify<P: Serialize>(&self, method: &str, params: P) -> Result<(), LspError> {
        if !self.is_alive() {
            return Err(LspError::NotRunning);
        }
        let body = serde_json::to_vec(&OutgoingNotification {
            jsonrpc: JSONRPC_VERSION,
            method,
            params,
        })
        .map_err(|e| LspError::Transport(format!("could not encode {method}: {e}")))?;
        self.write_frame(&body)
    }

    /// Ask the server to shut down, then make sure it did.
    ///
    /// The protocol's sequence is `shutdown` (a request) then `exit` (a notification), and
    /// it is worth following: a server that is asked properly flushes its caches, so the
    /// next start is fast instead of a cold rebuild. But it is bounded — a server busy
    /// inside `cargo check` will not answer, and a window that will not close because a
    /// background process is thinking is a worse bug than an ungraceful exit.
    pub fn shutdown(&self) {
        if self.is_alive() {
            let _: Result<serde_json::Value, _> =
                self.request(method::SHUTDOWN, serde_json::Value::Null, SHUTDOWN_GRACE);
            let _ = self.notify(method::EXIT, serde_json::Value::Null);
        }
        // Closing stdin is the second signal: a server blocked on reading its input exits
        // on end of stream even if it ignored `exit`.
        self.stdin.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.mark_dead("stopped");
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

    /// Mark the session dead and release every caller blocked on a response.
    ///
    /// Draining `pending` is the load-bearing half: without it, each waiting caller sits
    /// out its whole timeout for an answer that provably cannot arrive, and a crashed
    /// server turns into a UI that hangs for thirty seconds per feature.
    fn mark_dead(&self, reason: &str) {
        if !self.alive.swap(false, Ordering::SeqCst) {
            return; // already reported
        }
        let waiters: Vec<_> = {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            pending.drain().map(|(_, tx)| tx).collect()
        };
        for tx in waiters {
            // Dropping the sender is what wakes the caller with `Disconnected`.
            drop(tx);
        }
        self.handler.on_exit(reason);
    }

    fn write_frame(&self, body: &[u8]) -> Result<(), LspError> {
        // The guard is scoped tightly so it is released before `mark_dead` runs below —
        // that path calls back into the handler, and holding the write lock across a
        // callback is how a transport deadlocks.
        let written = {
            let mut guard = self.stdin.lock().unwrap_or_else(|p| p.into_inner());
            match guard.as_mut() {
                Some(stdin) => jsonrpc::write_frame(stdin, body),
                None => return Err(LspError::NotRunning),
            }
        };
        match written {
            Ok(()) => Ok(()),
            Err(e) => {
                // A broken pipe means the server is gone. Recording it here saves every
                // subsequent caller a full timeout.
                self.mark_dead("the connection to the language server was lost");
                Err(LspError::Transport(e.to_string()))
            }
        }
    }

    /// Complete a pending request, or note that nobody was waiting.
    fn deliver(&self, id: &RequestId, result: Result<serde_json::Value, ResponseError>) {
        let RequestId::Number(n) = id else {
            // We only ever send numeric ids, so a string id in a *response* is a server
            // bug; there is no caller it could belong to.
            return;
        };
        let tx = self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(n);
        if let Some(tx) = tx {
            let _ = tx.send(result); // the caller may have timed out and left
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Never leave an orphan: a rust-analyzer that outlives its window keeps a core
        // busy and a gigabyte resident.
        if let Some(child) = self.child.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The reader thread: parse frames until the stream ends, dispatching each one.
fn read_loop<R: BufRead>(weak: Weak<LspClient>, mut reader: R) {
    loop {
        // The peer name goes into the transport errors, so a desync says which process it was.
        let frame = match jsonrpc::read_frame(&mut reader, "the language server") {
            Ok(Some(body)) => body,
            Ok(None) => {
                if let Some(client) = weak.upgrade() {
                    client.mark_dead("the language server exited");
                }
                return;
            }
            Err(e) => {
                if let Some(client) = weak.upgrade() {
                    client.mark_dead(&format!("language server protocol error: {e}"));
                }
                return;
            }
        };
        // Upgrade per message: when the client has been dropped there is nobody to
        // dispatch to, and the process is already being killed.
        let Some(client) = weak.upgrade() else { return };

        let incoming: Incoming = match serde_json::from_slice(&frame) {
            Ok(v) => v,
            Err(e) => {
                // A single unparseable body is not a desync — the frame boundary was
                // honoured — so log it and keep reading rather than tearing the session
                // down over one message.
                eprintln!("[lsp {}] undecodable message: {e}", client.tag);
                continue;
            }
        };
        match incoming.classify() {
            Some(Message::Response { id, result }) => client.deliver(&id, result),
            Some(Message::Notification { method, params }) => {
                dispatch_notification(&client, &method, params)
            }
            Some(Message::Request { id, method, params }) => {
                // On a worker thread, not here: a handler that answered by calling back
                // into the server would be waiting for a response that only this thread
                // can deliver. See the module docs.
                let c = Arc::clone(&client);
                // Kept for the failure message — `method` itself moves into the closure.
                let label = method.clone();
                let spawned = std::thread::Builder::new()
                    .name(format!("lsp-req-{}", client.tag))
                    .spawn(move || {
                        let response = answer_request(&c, id, &method, params);
                        if let Ok(body) = serde_json::to_vec(&response) {
                            let _ = c.write_frame(&body);
                        }
                    });
                if spawned.is_err() {
                    eprintln!("[lsp {}] could not spawn a worker for {label}", client.tag);
                }
            }
            None => {}
        }
    }
}

/// Route a server notification to the handler. Runs on the reader thread — see the
/// no-blocking rule on [`ServerHandler`].
fn dispatch_notification(client: &LspClient, method: &str, params: serde_json::Value) {
    match method {
        method::PUBLISH_DIAGNOSTICS => {
            if let Ok(p) = serde_json::from_value(params) {
                client.handler.on_diagnostics(p);
            }
        }
        method::PROGRESS => {
            if let Ok(p) = serde_json::from_value(params) {
                client.handler.on_progress(p);
            }
        }
        method::SHOW_MESSAGE | method::LOG_MESSAGE => {
            if let Ok(p) = serde_json::from_value(params) {
                client.handler.on_message(p, method == method::LOG_MESSAGE);
            }
        }
        // `$/cancelRequest` from the server, telemetry, and each server's own
        // extensions. Nothing to do, and nothing to complain about: ignoring an unknown
        // notification is what the spec requires.
        _ => {}
    }
}

/// Produce our answer to one of the server's requests.
///
/// Every branch answers *something*. A server that gets no reply to a request waits
/// forever — and rust-analyzer's `workspace/configuration` is sent during startup, so an
/// unanswered one is a server stuck at "initializing" with no visible cause.
fn answer_request(
    client: &LspClient,
    id: RequestId,
    method: &str,
    params: serde_json::Value,
) -> OutgoingResponse {
    match method {
        // "I am about to start a long operation under this token." Consent is the only
        // answer; the progress itself arrives as `$/progress` notifications.
        method::WORK_DONE_PROGRESS_CREATE => OutgoingResponse::ok(id, serde_json::Value::Null),

        // Dynamic capability (un)registration. Accepted rather than refused: a server that
        // registers `didChangeWatchedFiles` and is told no may skip features. We do not
        // act on the registration — the document sync we already do covers the buffers the
        // editor owns — so this is an honest "noted", not a promise broken later.
        method::REGISTER_CAPABILITY | method::UNREGISTER_CAPABILITY => {
            OutgoingResponse::ok(id, serde_json::Value::Null)
        }

        // "What are your settings for these sections?" One `null` per requested item is
        // the protocol's way of saying "no client-side override" — the server then uses
        // the `initializationOptions` it already has.
        method::CONFIGURATION => {
            let count = params
                .get("items")
                .and_then(|i| i.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            let nulls = vec![serde_json::Value::Null; count];
            OutgoingResponse::ok(id, serde_json::Value::Array(nulls))
        }

        method::WORKSPACE_FOLDERS => match serde_json::to_value(&client.folders) {
            Ok(v) => OutgoingResponse::ok(id, v),
            Err(e) => OutgoingResponse::err(id, ERR_METHOD_NOT_FOUND, e.to_string()),
        },

        method::APPLY_EDIT => {
            let applied = match serde_json::from_value(params) {
                Ok(p) => client.handler.on_apply_edit(p),
                Err(_) => false,
            };
            OutgoingResponse::ok(id, serde_json::json!({ "applied": applied }))
        }

        // A modal question with buttons. Bennu does not put a server's dialog on screen
        // mid-edit, so the answer is "no action taken" — which the protocol spells `null`,
        // and which a server must already handle (a user can always dismiss).
        method::SHOW_MESSAGE_REQUEST => OutgoingResponse::ok(id, serde_json::Value::Null),

        other => OutgoingResponse::err(
            id,
            ERR_METHOD_NOT_FOUND,
            format!("bennu does not implement {other}"),
        ),
    }
}

/// The stderr drain: keep the tail, and mirror it to our own stderr.
///
/// Mirroring goes to **stderr** specifically. bennu-be's stdout is its IPC channel to the
/// shell, so a stray line on it desyncs the shell's framing — the same rule as everywhere
/// else in the backend, and one a language server's chatty log would break instantly.
fn stderr_loop<R: BufRead>(weak: Weak<LspClient>, reader: R, tag: &str) {
    for line in reader.lines() {
        let Ok(line) = line else { return };
        let Some(client) = weak.upgrade() else { return };
        eprintln!("[lsp {tag}] {line}");
        let mut tail = client.stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
        if tail.len() == STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transient_failure_is_distinguished_from_a_broken_one() {
        // What the user sees depends on this: a cancelled request means "no answer yet",
        // a decode failure means the feature is actually broken.
        assert!(LspError::Timeout(method::COMPLETION).is_transient());
        assert!(LspError::NotRunning.is_transient());
        assert!(LspError::Server(ResponseError {
            code: jsonrpc::ERR_CONTENT_MODIFIED,
            message: "content modified".into(),
            data: None,
        })
        .is_transient());
        assert!(!LspError::Decode("bad shape".into()).is_transient());
        assert!(!LspError::Unsupported(method::RENAME).is_transient());
        assert!(!LspError::Server(ResponseError {
            code: -32603,
            message: "internal error".into(),
            data: None,
        })
        .is_transient());
    }

    #[test]
    fn an_error_names_the_method_that_hung() {
        // "which request hung" is the whole diagnostic value of a timeout.
        let msg = LspError::Timeout(method::SEMANTIC_TOKENS_FULL).to_string();
        assert!(msg.contains("textDocument/semanticTokens/full"), "{msg}");
    }

    /// The server requests we must answer, and what we answer with. A missing reply here
    /// is a server that hangs at startup with nothing on screen to explain it.
    #[test]
    fn every_known_server_request_gets_an_answer() {
        struct Noop;
        impl ServerHandler for Noop {}
        // A client shell with no process behind it: `answer_request` only reads `folders`
        // and `handler`, so this exercises the routing without a spawn.
        let client = LspClient {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            stderr_tail: Mutex::new(VecDeque::new()),
            folders: vec![WorkspaceFolder {
                uri: "file:///p".to_string(),
                name: "p".to_string(),
            }],
            handler: Arc::new(Noop),
            tag: "test".to_string(),
        };
        let id = || RequestId::Number(1);

        let r = answer_request(&client, id(), method::WORK_DONE_PROGRESS_CREATE, serde_json::Value::Null);
        assert!(r.error.is_none() && r.result.is_some());

        let r = answer_request(&client, id(), method::REGISTER_CAPABILITY, serde_json::Value::Null);
        assert!(r.error.is_none(), "refusing registration makes servers drop features");

        // One null per requested section — a single null for a two-item request is a
        // shape error the server may reject.
        let r = answer_request(
            &client,
            id(),
            method::CONFIGURATION,
            serde_json::json!({ "items": [{ "section": "rust-analyzer" }, { "section": "x" }] }),
        );
        assert_eq!(r.result.unwrap(), serde_json::json!([null, null]));

        let r = answer_request(&client, id(), method::WORKSPACE_FOLDERS, serde_json::Value::Null);
        assert_eq!(r.result.unwrap(), serde_json::json!([{ "uri": "file:///p", "name": "p" }]));

        // The default handler applies nothing, and says so rather than staying silent.
        let r = answer_request(
            &client,
            id(),
            method::APPLY_EDIT,
            serde_json::json!({ "edit": { "changes": {} } }),
        );
        assert_eq!(r.result.unwrap(), serde_json::json!({ "applied": false }));

        let r = answer_request(&client, id(), method::SHOW_MESSAGE_REQUEST, serde_json::Value::Null);
        assert!(r.error.is_none(), "a dismissed dialog is null, not an error");

        // Anything else is answered with method-not-found — a reply, not silence.
        let r = answer_request(&client, id(), "server/somethingNew", serde_json::Value::Null);
        assert_eq!(r.error.unwrap().code, ERR_METHOD_NOT_FOUND);
    }

    #[test]
    fn a_request_on_a_dead_client_fails_immediately() {
        struct Noop;
        impl ServerHandler for Noop {}
        let client = LspClient {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(false),
            stderr_tail: Mutex::new(VecDeque::new()),
            folders: Vec::new(),
            handler: Arc::new(Noop),
            tag: "test".to_string(),
        };
        let r: Result<serde_json::Value, _> =
            client.request(method::HOVER, serde_json::Value::Null, Duration::from_secs(5));
        assert!(matches!(r, Err(LspError::NotRunning)), "no waiting out the timeout");
    }

    #[test]
    fn marking_dead_releases_every_waiting_caller() {
        // The reason this matters: without the drain, a crashed server turns into a UI
        // that hangs for one full timeout per feature the user touches.
        struct Recorder(Mutex<Vec<String>>);
        impl ServerHandler for Recorder {
            fn on_exit(&self, reason: &str) {
                self.0.lock().unwrap().push(reason.to_string());
            }
        }
        let handler = Arc::new(Recorder(Mutex::new(Vec::new())));
        let client = LspClient {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            stderr_tail: Mutex::new(VecDeque::new()),
            folders: Vec::new(),
            handler: Arc::clone(&handler) as Arc<dyn ServerHandler>,
            tag: "test".to_string(),
        };
        let (tx, rx) = mpsc::channel();
        client.pending.lock().unwrap().insert(7, tx);

        client.mark_dead("the language server exited");

        assert!(
            matches!(rx.recv_timeout(Duration::from_millis(50)), Err(mpsc::RecvTimeoutError::Disconnected)),
            "the waiter is woken, not left on the channel"
        );
        assert!(client.pending.lock().unwrap().is_empty());
        assert_eq!(handler.0.lock().unwrap().len(), 1);

        // Idempotent: a second report must not fire the callback again.
        client.mark_dead("again");
        assert_eq!(handler.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_response_for_an_unknown_id_is_dropped_quietly() {
        struct Noop;
        impl ServerHandler for Noop {}
        let client = LspClient {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            next_id: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            stderr_tail: Mutex::new(VecDeque::new()),
            folders: Vec::new(),
            handler: Arc::new(Noop),
            tag: "test".to_string(),
        };
        // The caller timed out and left; the late answer must not panic.
        client.deliver(&RequestId::Number(99), Ok(serde_json::Value::Null));
        // A string id cannot belong to any request we sent.
        client.deliver(&RequestId::Str("x".into()), Ok(serde_json::Value::Null));
    }
}
