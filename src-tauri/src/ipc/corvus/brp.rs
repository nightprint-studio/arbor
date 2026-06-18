//! `brp` (Bevy Remote Protocol) domain — handlers routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. The pure
//! BRP logic already lives in the Tauri-free [`corvus_brp`] crate
//! (`BrpRegistry`, `BrpStatus`, `BrpSession`); these handlers only hold the
//! `AppState` mutex and delegate, so there is **no crate extraction to do**.
//!
//! ## Structured errors over a stringly-typed seam
//!
//! `brp_connect` / `brp_call` need to tell the frontend *why* a call failed —
//! transport vs. protocol vs. an RPC error code the game attached — without
//! string-matching. The generic `rpc` seam flattens a handler's `Err` to a
//! plain wire string, which would drop the `kind`/`code`/`data` fields. So
//! these two handlers **never fail at the seam level for a BRP problem**:
//! protocol/transport/rpc failures ride home inside the `Ok` value as the
//! `err` arm of a discriminated [`BrpConnectOutcome`] / [`BrpCallOutcome`]
//! (tagged `outcome: "ok" | "err"`). Only a genuinely internal failure (a
//! poisoned `AppState` mutex) propagates as a seam-string `AppError` — the FE
//! store normalises that to `{ kind: "internal" }`. `brp_disconnect` /
//! `brp_status` return `AppError` directly: their only failure *is* the mutex.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use corvus_brp::prelude::{
    probe_capabilities, BrpClient, BrpError, BrpRegistry, BrpSession, BrpStatus, DEFAULT_ENDPOINT,
};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// Lock the BRP registry on `AppState`, mapping a poisoned mutex to the same
/// `AppError` the inline command produced.
fn lock_brp(state: &AppState) -> Result<std::sync::MutexGuard<'_, BrpRegistry>, AppError> {
    state.brp.lock().map_err(|e| {
        tracing::error!("brp mutex poisoned: {e}");
        AppError::MutexPoisoned("brp".into())
    })
}

// ---------------------------------------------------------------------------
// Structured error envelope + discriminated outcomes
// ---------------------------------------------------------------------------

/// A BRP-level failure surfaced as a serialisable envelope. Kept separate from
/// `AppError` so the frontend can distinguish "transport/protocol problem" from
/// "BRP returned an error code" without string-matching.
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

/// `brp_connect` result: the new session status, or the BRP error that aborted
/// the probe. Serialised internally-tagged so the FE reads it as a discriminated
/// union (`{ outcome: 'ok', status } | { outcome: 'err', error }`).
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BrpConnectOutcome {
    Ok { status: BrpStatus },
    Err { error: BrpCallError },
}

/// `brp_call` result: the raw JSON-RPC `result` payload, or the BRP error.
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BrpCallOutcome {
    Ok { value: Value },
    Err { error: BrpCallError },
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[corvus::handler]
fn brp_disconnect(state: &AppState) -> Result<BrpStatus, AppError> {
    let mut reg = lock_brp(state)?;
    reg.clear();
    Ok(BrpStatus::from_session(None))
}

#[corvus::handler]
fn brp_status(state: &AppState) -> Result<BrpStatus, AppError> {
    let reg = lock_brp(state)?;
    Ok(BrpStatus::from_session(reg.session()))
}

#[derive(Debug, Deserialize)]
pub struct BrpConnectParams {
    /// Optional override — `None` falls back to `DEFAULT_ENDPOINT`.
    pub endpoint: Option<String>,
    /// Per-request timeout. Same value used for the probe call and all
    /// follow-up calls bound to this session.
    pub timeout_ms: Option<u64>,
}

/// Probe the endpoint with `rpc.discover` and, on success, stash the session.
/// Replaces any previous session — plan decision #2 says singleton.
#[corvus::handler]
async fn brp_connect(
    state: &AppState,
    params: BrpConnectParams,
) -> Result<BrpConnectOutcome, AppError> {
    let endpoint = params.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let timeout = Duration::from_millis(params.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

    let client = match BrpClient::new(endpoint.clone(), timeout) {
        Ok(c) => c,
        Err(e) => return Ok(BrpConnectOutcome::Err { error: e.into() }),
    };
    // Probe rpc.discover (hard) + registry.schema (soft) and pin the resulting
    // capability matrix on the session so subsequent status() calls expose it
    // to plugins without a round-trip.
    let caps = match probe_capabilities(&client).await {
        Ok(c) => c,
        Err(e) => return Ok(BrpConnectOutcome::Err { error: e.into() }),
    };

    let session = BrpSession::new(endpoint, client).with_capabilities(caps);
    let status = BrpStatus::from_session(Some(&session));
    let mut reg = lock_brp(state)?;
    reg.set(session);
    Ok(BrpConnectOutcome::Ok { status })
}

#[derive(Debug, Deserialize)]
pub struct BrpCallParams {
    pub method: String,
    pub params: Option<Value>,
}

/// Raw JSON-RPC pass-through. The frontend / Lua side picks the method name
/// and shapes its own params — keeps the host thin.
#[corvus::handler]
async fn brp_call(state: &AppState, params: BrpCallParams) -> Result<BrpCallOutcome, AppError> {
    let client = {
        let reg = lock_brp(state)?;
        match reg.session() {
            Some(session) => session.client.clone(),
            None => {
                return Ok(BrpCallOutcome::Err {
                    error: BrpCallError {
                        kind: "not_connected",
                        message: "BRP not connected — call brp_connect first".into(),
                        code: None,
                        data: None,
                    },
                })
            }
        }
    };
    match client.call(&params.method, params.params).await {
        Ok(value) => Ok(BrpCallOutcome::Ok { value }),
        Err(e) => Ok(BrpCallOutcome::Err { error: e.into() }),
    }
}
