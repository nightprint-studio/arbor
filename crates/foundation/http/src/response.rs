//! An outbound response: a status, headers, and a body that is either bytes or a live
//! event stream.
//!
//! [`Body::Sse`] exists in phase one even though nothing uses it yet, and that is a
//! deliberate call: MCP's Streamable HTTP transport is defined as "answer with JSON *or*
//! with an event stream", and a server that can only do the first has to be reopened —
//! not extended — the day it needs to push. The channel is the whole mechanism; the
//! writer that drains it is twenty lines in [`crate::server`].

use tokio::sync::mpsc;

/// One Server-Sent Event. `data` is written verbatim, one `data:` line per newline.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Optional `event:` name. `None` leaves the client's default (`message`).
    pub event: Option<String>,
    /// Optional `id:` — clients echo the last one back as `Last-Event-ID` on reconnect.
    pub id: Option<String>,
    /// Payload. Multi-line payloads are split across `data:` lines per the spec.
    pub data: String,
}

impl SseEvent {
    /// An unnamed, un-ided event carrying `data`.
    pub fn data(data: impl Into<String>) -> Self {
        Self { event: None, id: None, data: data.into() }
    }

    /// Name this event.
    pub fn named(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Give this event an id.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The wire form, terminating blank line included.
    pub(crate) fn encode(&self) -> String {
        let mut out = String::new();
        if let Some(e) = &self.event {
            out.push_str("event: ");
            out.push_str(e);
            out.push('\n');
        }
        if let Some(i) = &self.id {
            out.push_str("id: ");
            out.push_str(i);
            out.push('\n');
        }
        // A payload containing newlines is several `data:` lines; the client rejoins
        // them with `\n`. Writing it raw would end the event at the first newline.
        for line in self.data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

/// What the response carries.
pub enum Body {
    /// A complete, length-known payload.
    Bytes(Vec<u8>),
    /// A live event stream. The connection stays open until the sender drops, and is
    /// then closed — an SSE response is never followed by another on the same socket.
    Sse(mpsc::Receiver<SseEvent>),
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Bytes(b) => write!(f, "Bytes({} bytes)", b.len()),
            Body::Sse(_) => write!(f, "Sse(..)"),
        }
    }
}

/// One outbound response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// Header name → value. `Content-Length` / `Content-Type` are set by the
    /// constructors; the writer adds `Date`-less minimal framing and nothing else.
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    /// A response with an explicit status and no body.
    pub fn status(status: u16) -> Self {
        Self { status, headers: Vec::new(), body: Body::Bytes(Vec::new()) }
    }

    /// `200` with a JSON body. The payload is already-serialized text — this crate
    /// stays JSON-library-free on purpose, so the caller owns the serializer.
    pub fn json(body: impl Into<String>) -> Self {
        Self::with_body(200, "application/json; charset=utf-8", body.into().into_bytes())
    }

    /// `200 text/plain`.
    pub fn text(body: impl Into<String>) -> Self {
        Self::with_body(200, "text/plain; charset=utf-8", body.into().into_bytes())
    }

    /// `200 text/html`.
    pub fn html(body: impl Into<String>) -> Self {
        Self::with_body(200, "text/html; charset=utf-8", body.into().into_bytes())
    }

    /// An arbitrary status + content type + payload.
    pub fn with_body(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), content_type.into())],
            body: Body::Bytes(body),
        }
    }

    /// An event stream. The writer sends the SSE headers immediately, then drains
    /// `rx` until it closes.
    pub fn sse(rx: mpsc::Receiver<SseEvent>) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("content-type".into(), "text/event-stream".into()),
                ("cache-control".into(), "no-store".into()),
            ],
            body: Body::Sse(rx),
        }
    }

    /// Add or replace a header.
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        let name = name.to_ascii_lowercase();
        let value = value.into();
        match self.headers.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => self.headers.push((name, value)),
        }
        self
    }

    /// Whether this response ends the connection regardless of what the client asked for.
    pub(crate) fn forces_close(&self) -> bool {
        matches!(self.body, Body::Sse(_))
    }
}

/// The reason phrase for the statuses this server emits. Unknown codes get a generic
/// phrase rather than a panic — the status number is what clients act on.
pub(crate) fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_splits_multiline_payloads() {
        let e = SseEvent::data("a\nb").named("tick").with_id("7");
        assert_eq!(e.encode(), "event: tick\nid: 7\ndata: a\ndata: b\n\n");
    }

    #[test]
    fn with_header_replaces_rather_than_duplicates() {
        let r = Response::json("{}").with_header("Content-Type", "application/problem+json");
        let cts: Vec<_> = r.headers.iter().filter(|(n, _)| n == "content-type").collect();
        assert_eq!(cts.len(), 1);
        assert_eq!(cts[0].1, "application/problem+json");
    }
}
