//! JSON-RPC 2.0 HTTP client for Bevy Remote Protocol.
//!
//! Pure HTTP only. Watch/SSE arrives in BRP Phase 2 (see plan memory).
//!
//! Every call goes through `BrpClient::call(method, params)` which serialises
//! a JSON-RPC 2.0 request, POSTs it, and returns the unwrapped `result` value
//! (or surfaces the `error` object as `BrpError::Rpc`). The id is monotonic
//! per client — useful only for debug since JSON-RPC over HTTP correlates
//! request↔response by transport round-trip.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Failure modes surfaced to callers. Kept separate from `crate::error::AppError`
/// so the BRP layer stays unaware of the wider app — the Tauri command layer
/// is what folds `BrpError` into `AppError`.
#[derive(Debug, Error)]
pub enum BrpError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("non-success status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid JSON-RPC response: {0}")]
    InvalidResponse(String),
    #[error("BRP error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
}

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
}

#[derive(Deserialize)]
struct RpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcErrorObject>,
}

#[derive(Deserialize)]
struct RpcErrorObject {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

/// Stateless except for an HTTP client + endpoint. Cloned cheaply (the
/// reqwest client is internally Arc'd).
#[derive(Clone)]
pub struct BrpClient {
    endpoint: String,
    http: reqwest::Client,
    next_id: std::sync::Arc<AtomicU64>,
}

impl BrpClient {
    /// Build a client. `endpoint` is the full BRP HTTP URL — e.g.
    /// `http://127.0.0.1:15702`. The reqwest timeout caps every call.
    pub fn new(endpoint: String, timeout: Duration) -> Result<Self, BrpError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("arbor-brp/0.1")
            .build()
            .map_err(|e| BrpError::Transport(format!("client build: {e}")))?;
        Ok(Self {
            endpoint,
            http,
            next_id: std::sync::Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Issue one JSON-RPC call and return the `result` payload. `params` may
    /// be `None` for parameter-less methods like `rpc.discover`.
    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, BrpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = RpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params: params.as_ref(),
        };
        let resp = self
            .http
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| BrpError::Transport(format!("send: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| BrpError::Transport(format!("read body: {e}")))?;
        if !status.is_success() {
            return Err(BrpError::Status {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: RpcResponse = serde_json::from_str(&text).map_err(|e| {
            BrpError::InvalidResponse(format!("parse: {e} (body: {})", truncate(&text, 200)))
        })?;
        if let Some(err) = parsed.error {
            return Err(BrpError::Rpc {
                code: err.code,
                message: err.message,
                data: err.data,
            });
        }
        Ok(parsed.result.unwrap_or(Value::Null))
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}
