//! Server-Sent Events: a request that stays open and keeps producing events.
//!
//! `sse_start` prepares the request exactly as a send does (variables, auth, the environment's protection),
//! spawns the stream on the runtime and **returns at once** — nothing blocks while the stream lives. Everything
//! the stream does arrives as an `hirundo:sse` event carrying the `stream_id`: `connecting`, `open`, `event`
//! (id, name, data, retry, time), `error`, `reconnecting` and `closed`. The stream reconnects on its own,
//! sending `Last-Event-ID` and waiting the `retry` the server asked for (3 s by default), until it is stopped
//! (`sse_stop`, which is `request_cancel` by another name) or it fails five times in a row. The format is read
//! by `hirundo-model::sse`.

use std::sync::Arc;
use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{Method, Prepared, SseParser};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Notify;

use crate::context::{build, EnvBrief, Target};
use crate::send::IN_FLIGHT;

const TOPIC: &str = "hirundo:sse";
const DEFAULT_RETRY_MS: u64 = 3000;
const MAX_FAILURES_IN_A_ROW: u32 = 5;

#[derive(Debug, Deserialize)]
pub struct StartArgs {
    pub target: Target,
    /// Names the stream: every event it produces carries it, and `sse_stop` takes it.
    pub stream_id: String,
    #[serde(default)]
    pub confirmed: bool,
    /// Resume after this event id (the window passes the last one it saw when it restarts a stream).
    #[serde(default)]
    pub last_event_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Started {
    Started,
    Unresolved { names: Vec<String>, missing_secrets: Vec<String> },
    NeedsConfirmation { env: Option<EnvBrief>, method: Method, url: String },
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Bytes into text without splitting a multi-byte character that straddles two chunks: what cannot be decoded
/// yet stays in `buf` for the next one.
pub fn decode_chunk(buf: &mut Vec<u8>, new: &[u8]) -> String {
    buf.extend_from_slice(new);
    match std::str::from_utf8(buf) {
        Ok(text) => {
            let out = text.to_string();
            buf.clear();
            out
        }
        Err(e) => {
            let valid = e.valid_up_to();
            // An incomplete character at the end waits; anything else wrong is replaced.
            if e.error_len().is_none() {
                let out = String::from_utf8_lossy(&buf[..valid]).into_owned();
                buf.drain(..valid);
                out
            } else {
                let out = String::from_utf8_lossy(buf).into_owned();
                buf.clear();
                out
            }
        }
    }
}

#[arbor_rpc::handler]
async fn sse_start(state: &HirundoState, args: StartArgs) -> Result<Started, String> {
    let built = build(state, &args.target, true)?;
    if !built.prepared.unresolved.is_empty() || !built.resolved.missing_secrets.is_empty() {
        return Ok(Started::Unresolved { names: built.prepared.unresolved, missing_secrets: built.resolved.missing_secrets });
    }
    if built.needs_confirmation && !args.confirmed {
        return Ok(Started::NeedsConfirmation { env: built.env, method: built.prepared.method, url: built.prepared.url });
    }
    let cancel = Arc::new(Notify::new());
    IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).insert(args.stream_id.clone(), Arc::clone(&cancel));
    let sink = state.event_sink();
    let id = args.stream_id.clone();
    tokio::spawn(async move {
        run(sink.as_ref(), built.prepared, &id, args.last_event_id, cancel).await;
        IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    });
    Ok(Started::Started)
}

/// Stop a stream. False when it had already ended.
#[arbor_rpc::handler]
fn sse_stop(_state: &HirundoState, stream_id: String) -> Result<bool, String> {
    let found = IN_FLIGHT.lock().unwrap_or_else(|p| p.into_inner()).get(&stream_id).cloned();
    Ok(found.map(|n| n.notify_one()).is_some())
}

fn emit(sink: &dyn EventSink, stream_id: &str, kind: &str, extra: serde_json::Value) {
    let mut payload = json!({ "stream_id": stream_id, "kind": kind, "at_ms": now_ms() });
    if let (Some(base), Some(more)) = (payload.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    sink.emit(TOPIC, payload);
}

async fn run(sink: &dyn EventSink, p: Prepared, stream_id: &str, mut last_id: Option<String>, cancel: Arc<Notify>) {
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .redirect(if p.follow_redirects { reqwest::redirect::Policy::limited(10) } else { reqwest::redirect::Policy::none() })
        .danger_accept_invalid_certs(!p.verify_tls)
        .user_agent(concat!("Hirundo/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => return emit(sink, stream_id, "closed", json!({ "reason": format!("could not build the HTTP client: {e}") })),
    };
    let mut retry_ms = DEFAULT_RETRY_MS;
    let mut failures = 0u32;

    loop {
        emit(sink, stream_id, "connecting", json!({ "last_event_id": last_id }));
        let mut request = client.request(if p.method == Method::Post { reqwest::Method::POST } else { reqwest::Method::GET }, &p.url);
        for (k, v) in &p.headers {
            request = request.header(k.as_str(), v.as_str());
        }
        if !p.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept")) {
            request = request.header("Accept", "text/event-stream");
        }
        request = request.header("Cache-Control", "no-cache");
        if let Some(id) = &last_id {
            request = request.header("Last-Event-ID", id.as_str());
        }
        if let Some(body) = &p.body {
            request = request.body(body.clone());
        }

        let sent = tokio::select! {
            r = request.send() => r,
            _ = cancel.notified() => return emit(sink, stream_id, "closed", json!({ "reason": "stopped" })),
        };
        match sent {
            Err(e) => {
                failures += 1;
                emit(sink, stream_id, "error", json!({ "message": e.to_string() }));
            }
            Ok(mut response) => {
                let status = response.status();
                if !status.is_success() || status.as_u16() == 204 {
                    // The server said no (or that there is nothing): trying again would not change it.
                    return emit(sink, stream_id, "closed", json!({ "reason": format!("the server answered {status}") }));
                }
                failures = 0;
                emit(sink, stream_id, "open", json!({ "status": status.as_u16() }));
                let mut parser = SseParser::new();
                let mut partial: Vec<u8> = Vec::new();
                loop {
                    let chunk = tokio::select! {
                        c = response.chunk() => c,
                        _ = cancel.notified() => return emit(sink, stream_id, "closed", json!({ "reason": "stopped" })),
                    };
                    match chunk {
                        Ok(Some(bytes)) => {
                            for ev in parser.feed(&decode_chunk(&mut partial, &bytes)) {
                                emit(sink, stream_id, "event", json!({ "id": ev.id, "event": ev.event, "data": ev.data, "retry": ev.retry }));
                            }
                            retry_ms = parser.retry_ms().unwrap_or(retry_ms);
                            if let Some(id) = parser.last_event_id() {
                                last_id = Some(id.to_string());
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            emit(sink, stream_id, "error", json!({ "message": e.to_string() }));
                            break;
                        }
                    }
                }
            }
        }
        if failures >= MAX_FAILURES_IN_A_ROW {
            return emit(sink, stream_id, "closed", json!({ "reason": format!("gave up after {MAX_FAILURES_IN_A_ROW} failed attempts") }));
        }
        emit(sink, stream_id, "reconnecting", json!({ "in_ms": retry_ms }));
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(retry_ms)) => {}
            _ = cancel.notified() => return emit(sink, stream_id, "closed", json!({ "reason": "stopped" })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_split_between_two_chunks_is_decoded_whole() {
        let euro = "€".as_bytes(); // three bytes
        let mut buf = Vec::new();
        assert_eq!(decode_chunk(&mut buf, &[b'a', euro[0], euro[1]]), "a");
        assert_eq!(decode_chunk(&mut buf, &[euro[2], b'b']), "€b");
        assert!(buf.is_empty());
    }

    #[test]
    fn bytes_that_are_not_utf8_are_replaced_not_stuck() {
        let mut buf = Vec::new();
        assert_eq!(decode_chunk(&mut buf, &[b'x', 0xFF, b'y']), "x\u{FFFD}y");
        assert!(buf.is_empty());
    }
}
