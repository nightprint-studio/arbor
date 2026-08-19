//! The MCP server: protocol dispatch, and the HTTP transport it rides on.
//!
//! ## Which transport, and why this one
//!
//! MCP defines stdio and Streamable HTTP. Arbor uses HTTP because the thing holding the
//! tools — the launcher — is *already running* when the client shows up. A stdio server
//! is spawned by its client, so serving stdio would mean a bridge process whose only job
//! is to relay into the live launcher. HTTP removes it.
//!
//! ## JSON, or a stream, per request
//!
//! Streamable HTTP lets a server answer a POST with either `application/json` or an SSE
//! stream. This server answers JSON unless three things line up: the request is a
//! `tools/call`, the client said it accepts `text/event-stream`, and it carried an
//! `_meta.progressToken`. Then the answer is a stream — `notifications/progress` while the
//! tool runs, the response last, and the connection closed after it.
//!
//! All three conditions are load-bearing. A request without a token **must not** be sent
//! progress (there is nothing to correlate it with), a client that did not offer to accept
//! a stream cannot read one, and no other method takes long enough to be worth the socket.
//!
//! The `GET` that opens a **standalone** server→client stream is served too, and carries
//! exactly one thing: `notifications/tools/list_changed`.
//!
//! That refusal used to be the right answer, and it stopped being one for a concrete
//! reason. A client asks for the tool list once, when it connects, and keeps it. Rebuild a
//! backend behind a running client — the normal state of a development machine — and the
//! two disagree with nothing to say so: the client offers tools that are gone and cannot
//! see the ones that arrived. `listChanged` is the protocol's answer, and it needs a stream
//! that exists before there is a request to answer.
//!
//! Still nothing else: no sampling, no server-initiated requests, no per-request roaming
//! onto this stream. One notification, sent when the host says the set moved.
//!
//! ## Stateless
//!
//! No `Mcp-Session-Id` is issued. The spec makes it optional, and the state an MCP
//! session would hold (which project is open, which backend is up) lives in the launcher
//! and outlives any one client. Adding session identity would mean two lifetimes to keep
//! in step for no gain.

use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use arbor_http::prelude::{error_response, Request, Response as HttpResponse, Server, SseEvent};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc};

use crate::catalog::ToolCatalog;
use crate::jsonrpc::{self, codes, Message, Response};
use crate::progress::{self, Progress};
use crate::resource::ResourceCatalog;
use crate::tool::CallToolResult;

/// The protocol revision this server implements.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";

/// Revisions it will speak if a client asks for one of them. A client asking for
/// anything else is answered with [`LATEST_PROTOCOL_VERSION`] and decides for itself
/// whether it can proceed — which is the negotiation the spec describes.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[LATEST_PROTOCOL_VERSION, "2025-03-26"];

/// How the server introduces itself in `initialize`.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    /// Free text handed to the model once, at connection time. The right place to say
    /// what this server is *for* and which order the tools want to be used in —
    /// guidance that would otherwise have to be repeated in every tool description.
    pub instructions: Option<String>,
}

/// Transport-level guards. Both are cheap and both matter on a port that any local
/// process — including a web page's JavaScript — can reach.
#[derive(Debug, Clone, Default)]
pub struct Guards {
    /// Shared secret. When set, every request must carry `Authorization: Bearer <token>`.
    pub token: Option<String>,
    /// The path the endpoint answers on. Anything else is a 404.
    pub path: String,
}

impl Guards {
    /// Guards for a loopback endpoint at `/mcp` with a required token.
    pub fn with_token(token: impl Into<String>) -> Self {
        Self { token: Some(token.into()), path: "/mcp".to_string() }
    }
}

/// A client that has introduced itself, and when.
///
/// ## What this can and cannot say
///
/// The transport is stateless — no `Mcp-Session-Id` is issued — so **only `initialize`
/// identifies anyone**. Every later request is anonymous: a `tools/call` cannot be
/// attributed to the client that made it, and a client that has gone away leaves nothing
/// behind to notice.
///
/// So this is a record of *introductions*, honestly labelled as one, and it answers the
/// question that actually gets asked — "has anything connected, and what is it?" — which
/// was previously unanswerable. Attributing individual calls would mean issuing session
/// ids and honouring their lifetime, which is a different feature with a different cost.
#[derive(Debug, Clone, Serialize)]
pub struct ClientRecord {
    /// What the client calls itself. Free text it chose; treat it as a label, not an
    /// identity — nothing verifies it.
    pub name: String,
    pub version: String,
    /// The protocol revision the handshake settled on.
    pub protocol: String,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
    /// Handshakes this run. A number climbing on its own is a client being restarted, or
    /// one reconnecting because something keeps dropping it.
    pub handshakes: u32,
}

/// The server. Cheap to clone through an `Arc`; holds no per-connection state.
pub struct McpServer<C: ToolCatalog> {
    catalog: Arc<C>,
    /// Fan-out to every open standalone stream.
    ///
    /// A broadcast rather than one channel: several clients may be connected, and each
    /// needs telling. Sending with no subscriber is not an error — the usual case is that
    /// nobody has opened a stream, and the host must not have to know that.
    notifications: broadcast::Sender<String>,
    /// Who has introduced themselves, in the order they first did.
    clients: Mutex<Vec<ClientRecord>>,
    /// Standalone streams open right now.
    streams: Arc<AtomicUsize>,
    /// Authenticated requests this run, and when the last one arrived.
    ///
    /// Separate from [`ClientRecord`] because it answers a different question with a
    /// different certainty. Identity comes only from `initialize`, so a client that
    /// connected before this process started — which is every client, after Arbor
    /// restarts, since a stateless transport gives it no reason to say hello again — makes
    /// calls that name nobody. Counting them is the difference between "nobody is there"
    /// and "somebody is there and has not introduced themselves".
    requests: AtomicUsize,
    last_request_ms: AtomicU64,
    /// Optional read-only context. Absent → the `resources` capability is not
    /// advertised and `resources/*` answers "method not found", which is the honest
    /// pair: a client must not be shown a picker nothing fills.
    resources: Option<Arc<dyn ResourceCatalog>>,
    info: ServerInfo,
    guards: Guards,
}

impl<C: ToolCatalog> McpServer<C> {
    pub fn new(catalog: Arc<C>, info: ServerInfo, guards: Guards) -> Self {
        let guards = Guards {
            path: if guards.path.is_empty() { "/mcp".to_string() } else { guards.path },
            ..guards
        };
        Self {
            catalog,
            resources: None,
            info,
            guards,
            notifications: broadcast::channel(NOTIFY_BUFFER).0,
            clients: Mutex::new(Vec::new()),
            streams: Arc::new(AtomicUsize::new(0)),
            requests: AtomicUsize::new(0),
            last_request_ms: AtomicU64::new(0),
        }
    }

    /// Offer read-only context alongside the tools.
    pub fn with_resources(mut self, resources: Arc<dyn ResourceCatalog>) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Serve until `shutdown` resolves, over an already-bound listener.
    ///
    /// Binding is the caller's job so it can publish the real port before the first
    /// request arrives — see [`arbor_http::Server::bind`].
    pub async fn serve<S>(self: Arc<Self>, http: Server, shutdown: S)
    where
        S: Future<Output = ()> + Send,
    {
        http.serve_with_shutdown(
            move |req: Request| {
                let me = Arc::clone(&self);
                async move { me.handle_http(req).await }
            },
            shutdown,
        )
        .await;
    }

    /// One HTTP request in, one out.
    pub async fn handle_http(self: Arc<Self>, req: Request) -> HttpResponse {
        // DNS-rebinding defence. A browser attaches `Origin`; a page on evil.example
        // that resolves to 127.0.0.1 would otherwise be able to drive this endpoint.
        // A non-browser client (Claude Code) sends no Origin at all, which is fine —
        // the token is what authenticates it.
        if let Some(origin) = req.header("origin") {
            if !is_local_origin(origin) {
                return error_response(403, "origin not allowed");
            }
        }

        if req.path != self.guards.path {
            return error_response(404, "not found");
        }

        if let Some(expected) = &self.guards.token {
            let presented = req
                .header("authorization")
                .and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")))
                .unwrap_or("");
            if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
                return error_response(401, "missing or invalid bearer token")
                    .with_header("www-authenticate", "Bearer");
            }
        }

        self.requests.fetch_add(1, Ordering::Relaxed);
        self.last_request_ms.store(now_ms(), Ordering::Relaxed);

        match req.method.as_str() {
            "POST" => self.handle_post(&req).await,
            "GET" => self.open_stream(&req),
            // Nothing to tear down — sessions are not issued. Acknowledge and move on.
            "DELETE" => HttpResponse::status(204),
            _ => error_response(405, "method not allowed").with_header("allow", "GET, POST, DELETE"),
        }
    }

    /// Everyone who has introduced themselves this run, first contact first.
    pub fn clients(&self) -> Vec<ClientRecord> {
        self.clients.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Authenticated requests this run, and when the last one arrived (0 = never).
    ///
    /// The honest floor of "is anything talking to this": it needs no identity, and it
    /// keeps counting for a client that connected before this process did.
    pub fn traffic(&self) -> (usize, u64) {
        (self.requests.load(Ordering::Relaxed), self.last_request_ms.load(Ordering::Relaxed))
    }

    /// Standalone streams open right now — the one thing here that IS live presence.
    ///
    /// Not attributable to a client: a `GET` carries no identity either. It answers "is
    /// anything actually listening", which is a different and simpler question.
    pub fn open_streams(&self) -> usize {
        self.streams.load(Ordering::Relaxed)
    }

    /// Fold one handshake into the record.
    fn record_handshake(&self, params: &Value, protocol: &str) {
        let info = params.get("clientInfo");
        let name = info
            .and_then(|i| i.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("unnamed client")
            .to_string();
        let version = info
            .and_then(|i| i.get("version"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let now = now_ms();

        let Ok(mut clients) = self.clients.lock() else { return };
        // Matched on name AND version, which is the closest thing to an identity a
        // stateless transport offers. Two copies of the same client collapse into one row
        // — stated in the type's docs rather than papered over with a guess.
        match clients.iter_mut().find(|c| c.name == name && c.version == version) {
            Some(seen) => {
                seen.last_seen_ms = now;
                seen.handshakes = seen.handshakes.saturating_add(1);
                seen.protocol = protocol.to_string();
            }
            None => clients.push(ClientRecord {
                name,
                version,
                protocol: protocol.to_string(),
                first_seen_ms: now,
                last_seen_ms: now,
                handshakes: 1,
            }),
        }
    }

    /// Tell every connected client that the tool set has moved.
    ///
    /// Fire-and-forget: a client that is not listening will ask again when it next
    /// connects, and one that is lagging gets the *next* notification — the message
    /// carries no state, only "ask again", so a missed one is never a lost update.
    pub fn notify_tools_changed(&self) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/list_changed",
        });
        let _ = self.notifications.send(notification.to_string());
    }

    /// The standalone server→client stream.
    ///
    /// One per `GET`, open until the client hangs up. Nothing is replayed onto a stream
    /// that opens late: the client has just listed the tools by connecting, so the only
    /// honest thing to send is what changes from here.
    fn open_stream(self: &Arc<Self>, req: &Request) -> HttpResponse {
        let accepts = req
            .header("accept")
            .is_some_and(|a| a.to_ascii_lowercase().contains("text/event-stream"));
        if !accepts {
            // The client asked for something this endpoint has no other form of. Not a
            // 405 — the method is fine, the representation is what we cannot provide.
            return error_response(406, "GET on this endpoint serves text/event-stream only")
                .with_header("allow", "GET, POST, DELETE");
        }

        let (tx, rx) = mpsc::channel::<SseEvent>(STREAM_BUFFER);
        let mut notifications = self.notifications.subscribe();
        // A guard, not a decrement at the end: the task has three exits and a count that
        // only falls on one of them is a count that drifts up for the life of the process.
        let open = StreamGuard::open(self.streams.clone());
        tokio::spawn(async move {
            let _open = open;
            loop {
                match notifications.recv().await {
                    Ok(message) => {
                        // The client hung up: end the task rather than hold a subscriber
                        // for a stream nobody is reading.
                        if tx.send(SseEvent::data(message)).await.is_err() {
                            return;
                        }
                    }
                    // Behind by more than the buffer. "Ask again" does not accumulate, so
                    // one message covers every one that was missed.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        HttpResponse::sse(rx)
    }

    async fn handle_post(self: &Arc<Self>, req: &Request) -> HttpResponse {
        let message = match jsonrpc::parse(&req.body_str()) {
            Ok(m) => m,
            // A malformed body is still answered with a JSON-RPC error object at HTTP
            // 200: the fault is in the payload, not in the exchange.
            Err(resp) => return HttpResponse::json(resp.to_json()),
        };

        if let Some(token) = stream_token(req, &message) {
            return self.stream_response(message, token);
        }

        let is_request = message.is_request();
        match self.handle_message(message, &Progress::none()).await {
            Some(resp) => HttpResponse::json(resp.to_json()),
            // A notification is acknowledged with 202 and an empty body.
            None if !is_request => HttpResponse::status(202),
            None => HttpResponse::status(202),
        }
    }

    /// Answer this request on an event stream instead of in one payload.
    ///
    /// Returns immediately with the headers; the work happens on a spawned task that owns
    /// the sending half, so the connection is already open — and already flushing progress
    /// — while the tool is still running. Dropping the sender is what ends the stream, and
    /// it happens on every exit from the task, including a panic.
    fn stream_response(self: &Arc<Self>, message: Message, token: Value) -> HttpResponse {
        // Bounded: a client that stops reading must slow the writer down, not let the
        // process buffer a build's worth of narration. Progress lines are dropped rather
        // than queued when it fills — see `Progress::send`.
        let (tx, rx) = mpsc::channel::<SseEvent>(STREAM_BUFFER);
        let me = Arc::clone(self);
        let responder = tx.clone();
        tokio::spawn(async move {
            let progress = Progress::to(token, tx);
            if let Some(response) = me.handle_message(message, &progress).await {
                // The response is the last thing on the stream, and the only thing on it
                // that the client is actually waiting for. `send`, not `try_send`: losing a
                // progress line is nothing, losing the answer is the whole call.
                let _ = responder.send(SseEvent::data(response.to_json())).await;
            }
        });
        HttpResponse::sse(rx)
    }

    /// Dispatch one JSON-RPC message. `None` for notifications, which are never
    /// answered.
    pub async fn handle_message(&self, message: Message, progress: &Progress) -> Option<Response> {
        let id = message.id.clone();
        let method = message.method.as_str();

        // Notifications first: they have no id, so they must not fall through to a
        // branch that tries to answer.
        if !message.is_request() {
            // `notifications/initialized`, `notifications/cancelled`, … — nothing here
            // keeps per-client state, so acknowledging by doing nothing is correct.
            return None;
        }
        let id = id.unwrap_or(Value::Null);

        let response = match method {
            "initialize" => Response::result(id, self.initialize_result(&message.params)),
            "ping" => Response::result(id, json!({})),
            "tools/list" => {
                let tools = self.catalog.list().await;
                match serde_json::to_value(json!({ "tools": tools })) {
                    Ok(v) => Response::result(id, v),
                    Err(e) => Response::error(id, codes::INTERNAL_ERROR, format!("tool list: {e}")),
                }
            }
            "tools/call" => self.tools_call(id, &message.params, progress).await,
            "resources/list" => match &self.resources {
                Some(catalog) => {
                    let resources = catalog.list().await;
                    Response::result(id, json!({ "resources": resources }))
                }
                None => Response::error(id, codes::METHOD_NOT_FOUND, "this server offers no resources"),
            },
            "resources/read" => self.resources_read(id, &message.params).await,
            other => Response::error(
                id,
                codes::METHOD_NOT_FOUND,
                format!("unknown method `{other}`"),
            ),
        };
        Some(response)
    }

    async fn resources_read(&self, id: Value, params: &Value) -> Response {
        let Some(catalog) = &self.resources else {
            return Response::error(id, codes::METHOD_NOT_FOUND, "this server offers no resources");
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return Response::error(id, codes::INVALID_PARAMS, "resources/read needs a `uri`");
        };
        match catalog.read(uri).await {
            Ok(contents) => Response::result(id, json!({ "contents": contents })),
            // A URI that does not resolve is the caller's mistake, not a server fault:
            // -32602 tells the client to fix the request rather than retry it.
            Err(e) => Response::error(id, codes::INVALID_PARAMS, e),
        }
    }

    fn initialize_result(&self, params: &Value) -> Value {
        // Echo the client's revision when we speak it, otherwise state ours and let it
        // decide — the spec's negotiation, not an error.
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = match requested {
            Some(v) if SUPPORTED_PROTOCOL_VERSIONS.contains(&v) => v,
            _ => LATEST_PROTOCOL_VERSION,
        };
        self.record_handshake(params, version);

        let mut capabilities = json!({ "tools": { "listChanged": true } });
        if self.resources.is_some() {
            capabilities["resources"] = json!({ "subscribe": false, "listChanged": false });
        }
        let mut result = json!({
            "protocolVersion": version,
            "capabilities": capabilities,
            "serverInfo": { "name": self.info.name, "version": self.info.version },
        });
        if let Some(instructions) = &self.info.instructions {
            result["instructions"] = json!(instructions);
        }
        result
    }

    async fn tools_call(&self, id: Value, params: &Value, progress: &Progress) -> Response {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Response::error(id, codes::INVALID_PARAMS, "tools/call needs a `name`");
        };
        // Absent arguments is legal for a no-argument tool.
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

        let outcome: CallToolResult = self.catalog.call(name, arguments, progress).await;
        match serde_json::to_value(outcome) {
            Ok(v) => Response::result(id, v),
            Err(e) => Response::error(id, codes::INTERNAL_ERROR, format!("tool result: {e}")),
        }
    }
}

/// How many events may sit unread before the writer waits.
///
/// Generous enough that a burst of narration never blocks the run that produced it, small
/// enough that a client which stopped reading cannot make this process hold a build's
/// output in memory.
const STREAM_BUFFER: usize = 64;

/// Holds the open-stream count up for as long as the stream lives.
struct StreamGuard(Arc<AtomicUsize>);

impl StreamGuard {
    fn open(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Milliseconds since the epoch. A clock that has gone backwards reports 0 rather than
/// panicking — a wrong timestamp on a status row is not worth taking the endpoint down.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// How many notifications a slow client may fall behind by before it is told to catch up
/// with a single one. Small on purpose — see the `Lagged` arm.
const NOTIFY_BUFFER: usize = 8;

/// The progress token to stream against, or `None` to answer in one payload.
///
/// Three conditions, all required — see the module docs for why each one is not optional.
fn stream_token(req: &Request, message: &Message) -> Option<Value> {
    if message.method != "tools/call" || !message.is_request() {
        return None;
    }
    let accepts_stream = req
        .header("accept")
        .is_some_and(|a| a.to_ascii_lowercase().contains("text/event-stream"));
    if !accepts_stream {
        return None;
    }
    progress::token_of(&message.params)
}

/// Whether an `Origin` belongs to this machine.
///
/// Deliberately strict: an exact loopback host, any port. `localhost.evil.com` and
/// `http://127.0.0.1.evil.com` both fail, which is the entire point of the check.
fn is_local_origin(origin: &str) -> bool {
    let rest = match origin.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") => rest,
        // `null` (a sandboxed iframe) and anything unparseable are not this machine.
        _ => return false,
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(h, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) { h } else { host }
    });
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

/// Compare without an early exit on the first differing byte.
///
/// The threat this actually addresses is modest — an attacker who can time loopback
/// requests has easier options — but a token compare is exactly the place where the
/// cheap version costs nothing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Tool, ToolAnnotations};
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl ToolCatalog for Fake {
        async fn list(&self) -> Vec<Tool> {
            vec![Tool {
                name: "demo".into(),
                title: Some("Demo".into()),
                description: "A demo tool.".into(),
                input_schema: json!({ "type": "object" }),
                annotations: ToolAnnotations {
                    read_only_hint: true,
                    destructive_hint: false,
                    idempotent_hint: true,
                    open_world_hint: false,
                },
            }]
        }

        async fn call(&self, name: &str, arguments: Value, progress: &Progress) -> CallToolResult {
            progress.send("working", None, None).await;
            match name {
                "demo" => CallToolResult::text(format!("called with {arguments}")),
                other => CallToolResult::error(format!("no such tool `{other}`")),
            }
        }
    }

    fn server(token: Option<&str>) -> Arc<McpServer<Fake>> {
        Arc::new(McpServer::new(
            Arc::new(Fake),
            ServerInfo {
                name: "arbor-test".into(),
                version: "0".into(),
                instructions: Some("Be brief.".into()),
            },
            Guards { token: token.map(str::to_string), path: "/mcp".into() },
        ))
    }

    fn post(body: &str, token: Option<&str>, origin: Option<&str>) -> Request {
        let mut headers = std::collections::HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        if let Some(t) = token {
            headers.insert("authorization".to_string(), format!("Bearer {t}"));
        }
        if let Some(o) = origin {
            headers.insert("origin".to_string(), o.to_string());
        }
        Request {
            method: "POST".into(),
            path: "/mcp".into(),
            query: String::new(),
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    /// A POST that offers to read a stream, the way a real client's does.
    fn streaming_post(body: &str) -> Request {
        let mut req = post(body, None, None);
        req.headers
            .insert("accept".to_string(), "application/json, text/event-stream".to_string());
        req
    }

    /// Drain a streamed response into the JSON-RPC messages it carried, in order.
    async fn stream_of(resp: arbor_http::Response) -> Vec<Value> {
        let arbor_http::Body::Sse(mut rx) = resp.body else { panic!("expected a stream") };
        let mut out = Vec::new();
        while let Some(event) = rx.recv().await {
            out.push(serde_json::from_str(&event.data).unwrap());
        }
        out
    }

    fn body_of(resp: arbor_http::Response) -> String {
        match resp.body {
            arbor_http::Body::Bytes(b) => String::from_utf8(b).unwrap(),
            arbor_http::Body::Sse(_) => panic!("unexpected stream"),
        }
    }

    #[tokio::test]
    async fn initialize_negotiates_the_protocol_version() {
        let s = server(None);
        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        // A revision we speak is echoed…
        assert_eq!(v["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(v["result"]["serverInfo"]["name"], "arbor-test");
        assert_eq!(v["result"]["instructions"], "Be brief.");

        // …one we don't is answered with ours.
        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["protocolVersion"], LATEST_PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn tools_list_and_call_round_trip() {
        let s = server(None);
        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, None, None)).await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["tools"][0]["name"], "demo");
        assert_eq!(v["result"]["tools"][0]["annotations"]["readOnlyHint"], true);

        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"demo","arguments":{"x":1}}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains(r#""x":1"#));
        assert!(v["result"].get("isError").is_none());
    }

    #[tokio::test]
    async fn a_failing_tool_is_a_successful_call_carrying_is_error() {
        let s = server(None);
        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        // Not a JSON-RPC error: the model is meant to read this and adapt.
        assert!(v.get("error").is_none(), "{v}");
        assert_eq!(v["result"]["isError"], true);
    }

    #[tokio::test]
    async fn unknown_methods_are_protocol_errors() {
        let s = server(None);
        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#, None, None)).await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["error"]["code"], codes::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn a_notification_is_acknowledged_without_a_body() {
        let s = server(None);
        let resp = s
            .clone().handle_http(post(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, None, None))
            .await;
        assert_eq!(resp.status, 202);
        assert_eq!(body_of(resp), "");
    }

    #[tokio::test]
    async fn the_token_is_required_when_configured() {
        let s = server(Some("secret"));
        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, None, None)).await;
        assert_eq!(resp.status, 401);

        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, Some("wrong"), None)).await;
        assert_eq!(resp.status, 401);

        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, Some("secret"), None)).await;
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn a_foreign_origin_is_refused() {
        let s = server(None);
        let resp = s
            .clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, None, Some("http://evil.example")))
            .await;
        assert_eq!(resp.status, 403);

        let resp = s
            .clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, None, Some("http://127.0.0.1:5173")))
            .await;
        assert_eq!(resp.status, 200);
    }

    struct FakeResources;

    #[async_trait]
    impl crate::resource::ResourceCatalog for FakeResources {
        async fn list(&self) -> Vec<crate::resource::Resource> {
            vec![crate::resource::Resource {
                uri: "arbor://project/demo".into(),
                name: "demo".into(),
                title: Some("Demo project".into()),
                description: None,
                mime_type: Some("text/markdown".into()),
            }]
        }

        async fn read(&self, uri: &str) -> Result<Vec<crate::resource::ResourceContents>, String> {
            if uri != "arbor://project/demo" {
                return Err(format!("no resource at `{uri}`"));
            }
            Ok(vec![crate::resource::ResourceContents {
                uri: uri.to_string(),
                mime_type: Some("text/markdown".into()),
                text: "# demo".into(),
            }])
        }
    }

    #[tokio::test]
    async fn resources_are_absent_until_offered() {
        let s = server(None);
        // Not advertised…
        let resp = s
            .clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, None, None))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert!(v["result"]["capabilities"].get("resources").is_none(), "{v}");

        // …and not answered, rather than answered emptily. An empty list would show the
        // user a picker with nothing in it and no way to tell why.
        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#, None, None)).await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["error"]["code"], codes::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn offered_resources_list_and_read() {
        let s = Arc::new(
            McpServer::new(
                Arc::new(Fake),
                ServerInfo { name: "t".into(), version: "0".into(), instructions: None },
                Guards { token: None, path: "/mcp".into() },
            )
            .with_resources(Arc::new(FakeResources)),
        );

        let resp = s
            .clone().handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, None, None))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["capabilities"]["resources"]["subscribe"], false);

        let resp = s.clone().handle_http(post(r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#, None, None)).await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["resources"][0]["uri"], "arbor://project/demo");
        assert_eq!(v["result"]["resources"][0]["mimeType"], "text/markdown");

        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"arbor://project/demo"}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["contents"][0]["text"], "# demo");
    }

    #[tokio::test]
    async fn an_unknown_uri_is_the_callers_mistake() {
        let s = Arc::new(
            McpServer::new(
                Arc::new(Fake),
                ServerInfo { name: "t".into(), version: "0".into(), instructions: None },
                Guards { token: None, path: "/mcp".into() },
            )
            .with_resources(Arc::new(FakeResources)),
        );
        let resp = s
            .clone().handle_http(post(
                r#"{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"arbor://nope"}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["error"]["code"], codes::INVALID_PARAMS);
        assert!(v["error"]["message"].as_str().unwrap().contains("arbor://nope"));
    }

    #[tokio::test]
    async fn a_call_that_asked_for_progress_is_answered_on_a_stream() {
        let s = server(None);
        let resp = s
            .clone()
            .handle_http(streaming_post(
                r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"demo","arguments":{},"_meta":{"progressToken":"tok"}}}"#,
            ))
            .await;
        assert_eq!(resp.status, 200);
        assert!(resp
            .headers
            .iter()
            .any(|(n, v)| n == "content-type" && v == "text/event-stream"));

        let messages = stream_of(resp).await;
        // Narration first, the answer last, and the stream ends after it.
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert_eq!(messages[0]["method"], "notifications/progress");
        assert_eq!(messages[0]["params"]["progressToken"], "tok");
        assert_eq!(messages[1]["id"], 9);
        assert!(messages[1]["result"]["content"][0]["text"].is_string());
    }

    #[tokio::test]
    async fn without_a_token_the_same_call_is_one_json_payload() {
        // The client did offer to read a stream — the missing token is what decides it, and
        // it must, since a progress notification with nothing to correlate it to is unusable.
        let s = server(None);
        let resp = s
            .clone()
            .handle_http(streaming_post(
                r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"demo"}}"#,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["id"], 10);
    }

    #[tokio::test]
    async fn a_client_that_did_not_offer_to_read_a_stream_is_not_sent_one() {
        let s = server(None);
        let resp = s
            .clone()
            .handle_http(post(
                r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"demo","_meta":{"progressToken":1}}}"#,
                None,
                None,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["id"], 11);
    }

    #[tokio::test]
    async fn only_tools_call_streams() {
        // `initialize` and the listings answer in microseconds; a socket held open for one
        // buys nothing and costs a connection.
        let s = server(None);
        let resp = s
            .clone()
            .handle_http(streaming_post(
                r#"{"jsonrpc":"2.0","id":12,"method":"tools/list","params":{"_meta":{"progressToken":1}}}"#,
            ))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["tools"][0]["name"], "demo");
    }

    #[tokio::test]
    async fn a_get_opens_the_stream_and_carries_the_tool_change() {
        let s = server(None);
        let mut req = post("", None, None);
        req.method = "GET".into();
        req.headers.insert("accept".into(), "text/event-stream".into());

        let resp = s.clone().handle_http(req).await;
        assert_eq!(resp.status, 200);
        let arbor_http::Body::Sse(mut rx) = resp.body else { panic!("expected a stream") };

        // Nothing is replayed: the stream carries what changes from here.
        s.notify_tools_changed();
        let event = rx.recv().await.expect("the notification must reach an open stream");
        let v: Value = serde_json::from_str(&event.data).unwrap();
        assert_eq!(v["method"], "notifications/tools/list_changed");
        assert!(v.get("id").is_none(), "a notification has no id: {v}");
    }

    #[tokio::test]
    async fn notifying_with_nobody_connected_is_not_an_error() {
        // The usual case, and the host must not have to know whether anyone is listening.
        server(None).notify_tools_changed();
    }

    #[tokio::test]
    async fn a_get_that_cannot_read_a_stream_is_refused_as_such() {
        let s = server(None);
        let mut req = post("", None, None);
        req.method = "GET".into();
        let resp = s.handle_http(req).await;
        // Not 405: the method is fine, the representation is what we have not got.
        assert_eq!(resp.status, 406);
    }

    #[tokio::test]
    async fn the_tool_list_now_announces_that_it_can_change() {
        let s = server(None);
        let resp = s
            .handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, None, None))
            .await;
        let v: Value = serde_json::from_str(&body_of(resp)).unwrap();
        assert_eq!(v["result"]["capabilities"]["tools"]["listChanged"], true);
    }

    #[tokio::test]
    async fn a_handshake_is_recorded_and_a_repeat_is_the_same_client() {
        let s = server(None);
        let hello = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","clientInfo":{"name":"claude-code","version":"2.1"}}}"#;
        s.clone().handle_http(post(hello, None, None)).await;
        s.clone().handle_http(post(hello, None, None)).await;

        let clients = s.clients();
        assert_eq!(clients.len(), 1, "the same client twice is one row: {clients:?}");
        assert_eq!(clients[0].name, "claude-code");
        assert_eq!(clients[0].version, "2.1");
        assert_eq!(clients[0].protocol, "2025-03-26");
        assert_eq!(clients[0].handshakes, 2);
        assert!(clients[0].last_seen_ms >= clients[0].first_seen_ms);
    }

    #[tokio::test]
    async fn a_client_that_gives_no_name_still_appears() {
        // `clientInfo` is not enforced by the transport, and a client that omits it is
        // still a client that connected — dropping the row would make the page lie.
        let s = server(None);
        s.clone()
            .handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, None, None))
            .await;
        assert_eq!(s.clients()[0].name, "unnamed client");
    }

    #[tokio::test]
    async fn traffic_is_counted_without_anyone_introducing_themselves() {
        // The case this exists for: after Arbor restarts, a client that connected before
        // has no reason to say hello again — a stateless transport does not make it — so
        // every call it makes names nobody. "Nobody is there" would be the wrong reading.
        let s = server(None);
        assert_eq!(s.traffic().0, 0);

        s.clone()
            .handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, None, None))
            .await;
        let (count, last) = s.traffic();
        assert_eq!(count, 1);
        assert!(last > 0);
        assert!(s.clients().is_empty(), "a call carries no identity");
    }

    #[tokio::test]
    async fn a_refused_request_is_not_traffic() {
        // A stranger failing the token is not "something is talking to this" — reading it
        // as such would turn a probe into reassurance.
        let s = server(Some("secret"));
        s.clone()
            .handle_http(post(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, Some("wrong"), None))
            .await;
        assert_eq!(s.traffic().0, 0);
    }

    #[tokio::test]
    async fn open_streams_are_counted_up_and_back_down() {
        let s = server(None);
        assert_eq!(s.open_streams(), 0);

        let mut req = post("", None, None);
        req.method = "GET".into();
        req.headers.insert("accept".into(), "text/event-stream".into());
        let resp = s.clone().handle_http(req).await;
        let arbor_http::Body::Sse(rx) = resp.body else { panic!("expected a stream") };
        assert_eq!(s.open_streams(), 1);

        // The client hangs up: the count must come back down without another notification
        // having to arrive to notice.
        drop(rx);
        s.notify_tools_changed();
        for _ in 0..50 {
            if s.open_streams() == 0 {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the stream count never fell back");
    }

    #[test]
    fn local_origin_is_not_fooled_by_a_lookalike_host() {
        assert!(is_local_origin("http://localhost:3000"));
        assert!(is_local_origin("http://127.0.0.1"));
        assert!(is_local_origin("https://[::1]:8080"));
        assert!(!is_local_origin("http://localhost.evil.com"));
        assert!(!is_local_origin("http://127.0.0.1.evil.com"));
        assert!(!is_local_origin("null"));
        assert!(!is_local_origin("file://"));
    }
}
