//! Tauri commands for Bevy Remote Protocol — Phase 1.0.
//!
//! Connect + raw call here; disconnect/status moved to the generic `corvus`
//! seam. SSE watch + editing arrive in later phases. The frontend talks to
//! these via `src/lib/ipc/brp.ts`; the Lua
//! `arbor.brp.*` namespace re-uses the same primitives directly from the
//! plugin host (no IPC round-trip).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::AppState;
use corvus_brp::prelude::{BrpClient, BrpError, BrpRegistry, BrpSession, BrpStatus, DEFAULT_ENDPOINT, probe_capabilities};
use crate::error::AppError;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Deserialize)]
pub struct BrpConnectParams {
    /// Optional override — `None` falls back to `DEFAULT_ENDPOINT`.
    pub endpoint: Option<String>,
    /// Per-request timeout. Same value used for the probe call and all
    /// follow-up calls bound to this session.
    pub timeout_ms: Option<u64>,
}

/// Surfaces a BRP-level failure as a serialisable envelope. We deliberately
/// don't fold this into `AppError` so the frontend can distinguish
/// "transport/protocol problem" from "BRP returned an error code" without
/// string-matching.
#[derive(Debug, Clone, Serialize)]
pub struct BrpCallError {
    pub kind: &'static str,
    pub message: String,
    /// Present only for `kind = "rpc"`.
    pub code: Option<i64>,
    /// Present only for `kind = "rpc"` when the game attached `data`.
    pub data: Option<Value>,
}

impl From<BrpError> for BrpCallError {
    fn from(e: BrpError) -> Self {
        match e {
            BrpError::Transport(m) => BrpCallError {
                kind: "transport",
                message: m,
                code: None,
                data: None,
            },
            BrpError::Status { status, body } => BrpCallError {
                kind: "status",
                message: format!("HTTP {status}: {body}"),
                code: Some(status as i64),
                data: None,
            },
            BrpError::InvalidResponse(m) => BrpCallError {
                kind: "invalid_response",
                message: m,
                code: None,
                data: None,
            },
            BrpError::Rpc { code, message, data } => BrpCallError {
                kind: "rpc",
                message,
                code: Some(code),
                data,
            },
        }
    }
}

/// Probe the endpoint with `rpc.discover` and, on success, stash the session.
/// Replaces any previous session — plan decision #2 says singleton.
#[tauri::command]
pub async fn brp_connect(
    state: State<'_, AppState>,
    params: BrpConnectParams,
) -> Result<BrpStatus, BrpCallError> {
    let endpoint = params.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let timeout = Duration::from_millis(params.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

    let client = BrpClient::new(endpoint.clone(), timeout)
        .map_err(BrpCallError::from)?;
    // Phase 1.2: probe rpc.discover (hard) + registry.schema (soft) and
    // pin the resulting capability matrix on the session so subsequent
    // status() calls expose it to plugins without a round-trip.
    let caps = probe_capabilities(&client)
        .await
        .map_err(BrpCallError::from)?;

    let session = BrpSession::new(endpoint, client).with_capabilities(caps);
    let status = BrpStatus::from_session(Some(&session));
    let mut reg = lock_brp(&state).map_err(call_err_from_app)?;
    reg.set(session);
    Ok(status)
}

// `brp_disconnect` / `brp_status` migrated to the generic seam
// (`ipc/corvus/brp.rs`); `brp_connect` / `brp_call` stay here — they are async
// and return the structured `BrpCallError` envelope, which the registry's
// stringly-typed error channel would flatten.

#[derive(Debug, Deserialize)]
pub struct BrpCallParams {
    pub method: String,
    pub params: Option<Value>,
}

/// Raw JSON-RPC pass-through. The frontend / Lua side picks the method name
/// and shapes its own params — keeps the host thin.
#[tauri::command]
pub async fn brp_call(
    state: State<'_, AppState>,
    params: BrpCallParams,
) -> Result<Value, BrpCallError> {
    let client = {
        let reg = lock_brp(&state).map_err(call_err_from_app)?;
        let session = reg
            .session()
            .ok_or_else(|| BrpCallError {
                kind: "not_connected",
                message: "BRP not connected — call brp_connect first".into(),
                code: None,
                data: None,
            })?;
        session.client.clone()
    };
    client
        .call(&params.method, params.params)
        .await
        .map_err(BrpCallError::from)
}

fn lock_brp<'a>(state: &'a State<'_, AppState>) -> Result<std::sync::MutexGuard<'a, BrpRegistry>, AppError> {
    state.brp.lock().map_err(|e| {
        tracing::error!("brp mutex poisoned: {e}");
        AppError::MutexPoisoned("brp".into())
    })
}

fn call_err_from_app(e: AppError) -> BrpCallError {
    BrpCallError {
        kind: "internal",
        message: e.to_string(),
        code: None,
        data: None,
    }
}
