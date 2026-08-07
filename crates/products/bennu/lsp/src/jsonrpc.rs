//! The LSP **base protocol**: JSON-RPC 2.0 bodies inside `Content-Length`-framed
//! headers, over a plain byte stream (a child process's stdin/stdout).
//!
//! Deliberately dumb: this module moves bytes and decides nothing. It does not know
//! what a request means, which requests exist, or who answers them — that is
//! [`crate::client`]'s job. What it owns is the one thing every LSP transport bug
//! comes from: the frame boundary.
//!
//! ```text
//! Content-Length: 42\r\n
//! \r\n
//! {"jsonrpc":"2.0","id":1,"method":"shutdown"}
//! ```
//!
//! Three rules the spec states and implementations forget:
//!
//! * the header block is ASCII and ends with an **empty** `\r\n` line;
//! * `Content-Length` counts **bytes**, not characters — a body with one `é` in it is
//!   longer than its `chars().count()`, so the body is read as bytes and decoded
//!   after;
//! * header names are **case-insensitive** (`content-length` is legal, and some
//!   servers send it).
//!
//! A malformed frame is a protocol desync, not a recoverable per-message error: once
//! the reader has lost the boundary every subsequent read is garbage. So framing
//! errors surface as [`io::Error`] and the caller's move is to declare the session
//! dead, which is what [`crate::client`] does.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

/// The header that carries the body length. Compared case-insensitively.
const CONTENT_LENGTH: &str = "content-length";

/// A hard ceiling on one message's body, as a defence against a desynced stream
/// turning a bogus length into a multi-gigabyte allocation.
///
/// Generous on purpose: a `textDocument/semanticTokens/full` for a 20k-line file, or
/// a `workspace/symbol` answer on a large Cargo workspace, is legitimately megabytes.
/// This is a sanity bound, not a policy.
const MAX_BODY: usize = 128 * 1024 * 1024;

/// Read one framed message body from `reader`.
///
/// `Ok(None)` is a clean end of stream — the server exited — and is the normal way a
/// reader loop terminates. `Err` means the framing itself was violated: the caller
/// must not try to read again.
pub fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            // EOF. Clean only *between* messages: mid-header it means the server died
            // with a half-written frame, which the caller should hear about as an error
            // rather than as an orderly shutdown.
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "language server closed its output mid-header",
                ))
            };
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of the header block
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed LSP header line: {trimmed:?}"),
            ));
        };
        if name.trim().eq_ignore_ascii_case(CONTENT_LENGTH) {
            content_length = Some(value.trim().parse::<usize>().map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("bad Content-Length: {e}"))
            })?);
        }
        // Every other header (`Content-Type`, and whatever a server invents) is
        // ignored: the spec fixes the charset at UTF-8 and nothing else is actionable.
    }

    let len = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "LSP frame without a Content-Length header")
    })?;
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("LSP frame claims {len} bytes — refusing (stream is probably desynced)"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Write one framed message body to `writer` and flush it.
///
/// The flush is not optional: a language server is a request/response peer, so a body
/// sitting in our buffer is a request the server never sees and a caller that blocks
/// until it times out.
pub fn write_message<W: Write>(writer: &mut W, body: &[u8]) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body)?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// JSON-RPC message shapes
// ---------------------------------------------------------------------------

/// A JSON-RPC request id. The spec allows a number or a string; we only ever *send*
/// numbers, but a server's own requests to us can carry either and must be echoed back
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    Str(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(n) => write!(f, "{n}"),
            RequestId::Str(s) => write!(f, "{s}"),
        }
    }
}

/// An error object in a JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

/// The JSON-RPC error code for "the request was cancelled", which servers use freely
/// (rust-analyzer cancels in-flight requests whenever the document changes under them).
/// Worth naming because it is the one error the caller should treat as "ask again",
/// not as "this feature is broken".
pub const ERR_REQUEST_CANCELLED: i64 = -32800;
/// Same idea: rust-analyzer answers with this while it is still loading the workspace.
pub const ERR_CONTENT_MODIFIED: i64 = -32801;

impl ResponseError {
    /// Whether this failure is transient — the request raced an edit or arrived before
    /// the server was ready — so the honest answer to the user is "nothing yet" rather
    /// than an error banner.
    pub fn is_transient(&self) -> bool {
        matches!(self.code, ERR_REQUEST_CANCELLED | ERR_CONTENT_MODIFIED)
    }
}

/// Anything arriving from the server, decoded far enough to classify it.
///
/// One permissive struct rather than an enum per shape, because JSON-RPC's three
/// message kinds are distinguished by which fields are *present*, and serde's untagged
/// enums decide that by trial deserialization — which turns a response carrying a
/// `method`-shaped result into a coin flip. Classification is explicit instead
/// ([`Incoming::classify`]).
#[derive(Debug, Deserialize)]
pub struct Incoming {
    #[serde(default)]
    pub id: Option<RequestId>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<ResponseError>,
}

/// What an [`Incoming`] actually is.
#[derive(Debug)]
pub enum Message {
    /// The server is asking *us* something and expects a response with this id.
    Request { id: RequestId, method: String, params: serde_json::Value },
    /// The server is telling us something; no response is expected or allowed.
    Notification { method: String, params: serde_json::Value },
    /// The answer to a request we sent.
    Response { id: RequestId, result: Result<serde_json::Value, ResponseError> },
}

impl Incoming {
    /// Classify by field presence, per JSON-RPC 2.0: `method` + `id` is a request,
    /// `method` alone is a notification, `id` alone is a response.
    pub fn classify(self) -> Option<Message> {
        let params = self.params.unwrap_or(serde_json::Value::Null);
        match (self.id, self.method) {
            (Some(id), Some(method)) => Some(Message::Request { id, method, params }),
            (None, Some(method)) => Some(Message::Notification { method, params }),
            (Some(id), None) => {
                let result = match self.error {
                    Some(e) => Err(e),
                    // A response with neither `result` nor `error` is illegal, but
                    // `result: null` is both legal and extremely common (every
                    // "nothing here" answer), and the two are indistinguishable after
                    // serde. Treating absent as null is the reading that keeps those
                    // working.
                    None => Ok(self.result.unwrap_or(serde_json::Value::Null)),
                };
                Some(Message::Response { id, result })
            }
            // No method and no id: not a JSON-RPC message at all.
            (None, None) => None,
        }
    }
}

/// A request we send to the server.
#[derive(Debug, Serialize)]
pub struct OutgoingRequest<'a, P> {
    pub jsonrpc: &'static str,
    pub id: i64,
    pub method: &'a str,
    pub params: P,
}

/// A notification we send to the server (no id, no answer).
#[derive(Debug, Serialize)]
pub struct OutgoingNotification<'a, P> {
    pub jsonrpc: &'static str,
    pub method: &'a str,
    pub params: P,
}

/// Our answer to one of the server's requests.
#[derive(Debug, Serialize)]
pub struct OutgoingResponse {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

/// The protocol version string every message carries.
pub const JSONRPC_VERSION: &str = "2.0";

impl OutgoingResponse {
    /// A successful answer.
    pub fn ok(id: RequestId, result: serde_json::Value) -> Self {
        Self { jsonrpc: JSONRPC_VERSION, id, result: Some(result), error: None }
    }

    /// A failure answer. Used for the server requests we deliberately don't implement:
    /// answering "method not found" is protocol-correct, whereas silence makes a server
    /// wait forever for a reply that will never come.
    pub fn err(id: RequestId, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            result: None,
            error: Some(ResponseError { code, message: message.into(), data: None }),
        }
    }
}

/// JSON-RPC's "no such method" code — our answer to a server request we don't handle.
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn read_all(input: &[u8]) -> Vec<Vec<u8>> {
        let mut r = BufReader::new(input);
        let mut out = Vec::new();
        while let Some(body) = read_message(&mut r).expect("framing") {
            out.push(body);
        }
        out
    }

    #[test]
    fn reads_back_what_it_writes() {
        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, br#"{"jsonrpc":"2.0","id":1}"#).unwrap();
        write_message(&mut buf, br#"{"jsonrpc":"2.0","id":2}"#).unwrap();
        let frames = read_all(&buf);
        assert_eq!(frames.len(), 2, "two frames round-tripped");
        assert_eq!(frames[1], br#"{"jsonrpc":"2.0","id":2}"#);
    }

    #[test]
    fn content_length_counts_bytes_not_characters() {
        // `é` is two bytes and one char. A length in characters would truncate the body
        // and desync every following frame — the classic LSP transport bug.
        let body = r#"{"m":"é"}"#;
        assert_ne!(body.len(), body.chars().count(), "the fixture must be multi-byte");
        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, body.as_bytes()).unwrap();
        assert!(
            String::from_utf8_lossy(&buf).starts_with(&format!("Content-Length: {}", body.len())),
            "header counts bytes"
        );
        assert_eq!(read_all(&buf)[0], body.as_bytes());
    }

    #[test]
    fn header_name_is_case_insensitive_and_extra_headers_are_ignored() {
        let raw = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}";
        assert_eq!(read_all(raw)[0], b"{}");
    }

    #[test]
    fn clean_eof_between_messages_is_not_an_error() {
        let mut r = BufReader::new(&b""[..]);
        assert!(read_message(&mut r).unwrap().is_none(), "end of stream, not a failure");
    }

    #[test]
    fn eof_mid_header_is_an_error() {
        // The server died with a frame half-written: the caller must not read on.
        let mut r = BufReader::new(&b"Content-Length: 10\r\n"[..]);
        assert!(read_message(&mut r).is_err());
    }

    #[test]
    fn a_frame_without_a_length_is_rejected() {
        let mut r = BufReader::new(&b"Content-Type: x\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).is_err());
    }

    #[test]
    fn an_absurd_length_is_refused_rather_than_allocated() {
        let raw = format!("Content-Length: {}\r\n\r\n", MAX_BODY + 1);
        let mut r = BufReader::new(raw.as_bytes());
        assert!(read_message(&mut r).is_err());
    }

    #[test]
    fn classification_follows_field_presence() {
        let req: Incoming =
            serde_json::from_str(r#"{"id":1,"method":"window/workDoneProgress/create"}"#).unwrap();
        assert!(matches!(req.classify(), Some(Message::Request { .. })));

        let note: Incoming =
            serde_json::from_str(r#"{"method":"textDocument/publishDiagnostics"}"#).unwrap();
        assert!(matches!(note.classify(), Some(Message::Notification { .. })));

        let resp: Incoming = serde_json::from_str(r#"{"id":7,"result":null}"#).unwrap();
        match resp.classify() {
            Some(Message::Response { id, result }) => {
                assert_eq!(id, RequestId::Number(7));
                assert_eq!(result.unwrap(), serde_json::Value::Null);
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn a_response_with_neither_result_nor_error_reads_as_null() {
        // Illegal per spec, sent in practice, and indistinguishable from `result: null`
        // once serde is done. Reading it as null is what keeps "nothing here" answers working.
        let resp: Incoming = serde_json::from_str(r#"{"id":3}"#).unwrap();
        match resp.classify() {
            Some(Message::Response { result, .. }) => {
                assert_eq!(result.unwrap(), serde_json::Value::Null)
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn an_error_response_carries_the_error() {
        let resp: Incoming = serde_json::from_str(
            r#"{"id":3,"error":{"code":-32800,"message":"content modified"}}"#,
        )
        .unwrap();
        match resp.classify() {
            Some(Message::Response { result: Err(e), .. }) => {
                assert!(e.is_transient(), "a cancellation is not a broken feature");
            }
            other => panic!("expected an error response, got {other:?}"),
        }
    }

    #[test]
    fn a_string_id_survives_the_round_trip() {
        // We only ever send numbers, but a server's own requests may use strings and the
        // id has to come back byte-identical or the server never matches our answer.
        let req: Incoming = serde_json::from_str(r#"{"id":"ra-42","method":"x"}"#).unwrap();
        let Some(Message::Request { id, .. }) = req.classify() else { panic!("a request") };
        let body = serde_json::to_string(&OutgoingResponse::ok(id, serde_json::Value::Null)).unwrap();
        assert!(body.contains(r#""id":"ra-42""#), "{body}");
    }
}
