//! JSON-RPC 2.0 — the envelope MCP speaks inside.
//!
//! One deliberate narrowing: **batches are refused**. JSON-RPC allows an array of
//! messages, MCP's 2025-06-18 revision removed support for it, and an untested batch
//! path is worse than an honest error. A client that sends one gets told so.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Standard JSON-RPC error codes, plus the range MCP leaves to the server.
pub mod codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// An inbound message. A missing `id` makes it a notification: it is acted on and
/// never answered.
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Message {
    /// Whether this message expects a response.
    pub fn is_request(&self) -> bool {
        self.id.is_some()
    }
}

/// An outbound response. Exactly one of `result` / `error` is present.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

/// The error half of a response.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Response {
    /// A success.
    pub fn result(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }

    /// A protocol-level failure.
    ///
    /// Note what this is *not* for: a tool that ran and failed is a **successful**
    /// JSON-RPC call carrying `isError: true`, because the model is supposed to read
    /// the failure and react to it. Reserve this for malformed requests, unknown
    /// methods, and the server breaking.
    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(ErrorObject { code, message: message.into(), data: None }),
        }
    }

    /// Attach structured detail to an error.
    pub fn with_data(mut self, data: Value) -> Self {
        if let Some(e) = self.error.as_mut() {
            e.data = Some(data);
        }
        self
    }

    /// Serialize, falling back to a hand-built envelope if that somehow fails — an
    /// unanswerable request is the one outcome a JSON-RPC server must never produce.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            json!({
                "jsonrpc": "2.0",
                "id": self.id,
                "error": { "code": codes::INTERNAL_ERROR, "message": format!("response serialization failed: {e}") }
            })
            .to_string()
        })
    }
}

/// Parse one inbound body.
pub fn parse(body: &str) -> Result<Message, Response> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| Response::error(Value::Null, codes::PARSE_ERROR, format!("invalid JSON: {e}")))?;

    if value.is_array() {
        return Err(Response::error(
            Value::Null,
            codes::INVALID_REQUEST,
            "batched requests are not supported — send one message per request",
        ));
    }

    serde_json::from_value::<Message>(value).map_err(|e| {
        Response::error(Value::Null, codes::INVALID_REQUEST, format!("not a JSON-RPC message: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_has_an_id_a_notification_does_not() {
        let req = parse(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert!(req.is_request());
        let note = parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert!(!note.is_request());
    }

    #[test]
    fn batches_are_refused_rather_than_half_handled() {
        let err = parse(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#).unwrap_err();
        assert_eq!(err.error.unwrap().code, codes::INVALID_REQUEST);
    }

    #[test]
    fn garbage_is_a_parse_error_with_a_null_id() {
        let err = parse("not json").unwrap_err();
        assert_eq!(err.id, Value::Null);
        assert_eq!(err.error.unwrap().code, codes::PARSE_ERROR);
    }

    #[test]
    fn a_success_carries_no_error_key() {
        let json = Response::result(json!(1), json!({"ok": true})).to_json();
        assert!(json.contains(r#""result""#));
        assert!(!json.contains(r#""error""#));
    }
}
