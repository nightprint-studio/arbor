//! The LSP **message bodies**: JSON-RPC 2.0 requests, responses and notifications.
//!
//! The envelope they travel in — `Content-Length`-framed headers over a byte stream — is
//! [`bennu_framed`], because DAP uses the identical envelope with entirely different bodies. It
//! lived here while the language server was its only consumer; the debugger made it two, and a
//! second copy of a frame reader is a second place for a desync bug to be fixed in.
//!
//! What is left here is the part that is genuinely LSP's: the `jsonrpc: "2.0"` shapes, the request
//! id, and the error object.

use serde::{Deserialize, Serialize};

// The envelope, re-exported so a call site inside this crate keeps reading as one transport.
pub use bennu_framed::{read_message as read_frame, write_message as write_frame, MAX_BODY};

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

    // The framing tests live in `bennu-framed`, which owns the envelope. What is tested here is what
    // is left: the JSON-RPC bodies.

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
