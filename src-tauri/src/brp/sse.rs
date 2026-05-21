//! Server-Sent Events client for BRP `*+watch` streams.
//!
//! BRP 0.18's `RemoteHttpPlugin` answers a watch-method POST with
//! `Content-Type: text/event-stream` and keeps the connection open. Each
//! mutation in the watched scope is delivered as a single SSE event whose
//! payload is the JSON-RPC response body, e.g.
//!
//! ```text
//! data: {"jsonrpc":"2.0","id":1,"result":{...}}
//!
//! data: {"jsonrpc":"2.0","id":1,"result":{...}}
//! ```
//!
//! We don't pull in `eventsource-stream` — the framing is trivial enough
//! that hand-parsing keeps the dep graph slim. Only the `data:` field is
//! honoured (`event:` / `id:` / `retry:` are unused by BRP).
//!
//! The stream runs in its own task; callers obtain an `AbortHandle` they
//! park in `BrpRegistry::watches` and call when the user invokes
//! `arbor.brp.unwatch` or the host tears the session down.
//!
//! NB: the streaming task uses its own `reqwest::Client` built without a
//! request timeout — the timeout on `BrpClient`'s sync client would clip the
//! long-lived response after a few seconds.

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;

/// Discrete events surfaced by `run_watch_stream`. The owner side maps each
/// into a Lua callback invocation. The variants are deliberately verbose so
/// plugins can render "Open" / "Reconnecting" / "Closed" hints without
/// re-deriving state from a single payload.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// Connection accepted (first byte of the SSE body received). Fired
    /// before any `Data` event so the plugin can clear a "connecting…" UI.
    Open,
    /// One unwrapped `result` payload from the BRP server.
    Data(Value),
    /// The server returned an explicit JSON-RPC error inside the stream
    /// (e.g. the entity disappeared during a `world.get_components+watch`).
    /// The stream may still continue — BRP keeps emitting events for the
    /// same subscription.
    RpcError {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    /// Transport, HTTP, or framing failure. `Close` always follows.
    Error(String),
    /// Final event; the task has exited and the registered subscription is
    /// no longer firing.
    Close,
}

/// Spin a long-running SSE stream against `endpoint` for the given JSON-RPC
/// `method` + `params` and dispatch every parsed event through `on_event`.
///
/// Designed to be `tokio::spawn`-ed; the caller keeps the JoinHandle's
/// `abort_handle()` to cancel early. On abort the task drops mid-poll — no
/// `Close` event is fired in that case (the unwatch caller already knows
/// the subscription is gone).
pub async fn run_watch_stream(
    endpoint: String,
    method: String,
    params: Option<Value>,
    on_event: impl Fn(WatchEvent) + Send + Sync + 'static,
) {
    // Dedicated client — no `.timeout(…)`, since the request lives as long
    // as the watch. `connect_timeout` still bounds the initial handshake so
    // a dead endpoint fails fast instead of hanging for minutes.
    let http = match reqwest::Client::builder()
        .user_agent("arbor-brp-watch/0.1")
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            on_event(WatchEvent::Error(format!("client build: {e}")));
            on_event(WatchEvent::Close);
            return;
        }
    };

    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let resp = match http
        .post(&endpoint)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .json(&request_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            on_event(WatchEvent::Error(format!("send: {e}")));
            on_event(WatchEvent::Close);
            return;
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        on_event(WatchEvent::Error(format!(
            "HTTP {}: {}",
            status.as_u16(),
            truncate(&body, 200)
        )));
        on_event(WatchEvent::Close);
        return;
    }

    on_event(WatchEvent::Open);

    let mut stream = resp.bytes_stream();
    // Accumulator across chunks — SSE frames are delimited by a blank
    // line (`\n\n` or `\r\n\r\n`) which may straddle TCP chunks.
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                on_event(WatchEvent::Error(format!("read: {e}")));
                break;
            }
        };
        // BRP's SSE body is JSON wrapped in `data:` fields → always UTF-8.
        // If a chunk straddles a multi-byte code point we'd lose it; in
        // practice every BRP frame is followed by `\n\n` so chunks line up.
        match std::str::from_utf8(&bytes) {
            Ok(s) => buf.push_str(s),
            Err(_) => continue,
        }
        // Split off complete frames. Tolerate both `\n\n` and `\r\n\r\n`
        // separators — the BRP server emits `\n\n` today but the SSE spec
        // technically allows both, and we shouldn't break if Bevy retunes
        // its framing later.
        loop {
            let (sep_idx, sep_len) = match (buf.find("\r\n\r\n"), buf.find("\n\n")) {
                (Some(a), Some(b)) if a < b => (a, 4),
                (Some(a), None) => (a, 4),
                (_, Some(b)) => (b, 2),
                (None, None) => break,
            };
            let frame: String = buf.drain(..sep_idx + sep_len).collect();
            if let Some(payload) = parse_sse_data(&frame) {
                dispatch_payload(&on_event, payload);
            }
        }
    }

    on_event(WatchEvent::Close);
}

/// Pull every `data:` line out of an SSE frame and concatenate them — the
/// spec says multiple `data:` fields in one event join with `\n`. Returns
/// `None` if the frame has no data (heartbeat / comment-only frames).
fn parse_sse_data(frame: &str) -> Option<String> {
    let mut out = String::new();
    for line in frame.lines() {
        let Some(rest) = line.strip_prefix("data:") else { continue };
        if !out.is_empty() {
            out.push('\n');
        }
        // The spec strips one optional leading space after the colon.
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        out.push_str(rest);
    }
    if out.is_empty() { None } else { Some(out) }
}

fn dispatch_payload(on_event: &(impl Fn(WatchEvent) + Send + Sync), payload: String) {
    let parsed: Value = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(e) => {
            on_event(WatchEvent::Error(format!("parse: {e}")));
            return;
        }
    };
    if let Some(err) = parsed.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("(no message)")
            .to_string();
        let data = err.get("data").cloned();
        on_event(WatchEvent::RpcError { code, message, data });
        return;
    }
    let result = parsed.get("result").cloned().unwrap_or(Value::Null);
    on_event(WatchEvent::Data(result));
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}
