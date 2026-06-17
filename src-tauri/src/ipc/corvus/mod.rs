//! In-process `corvus` backend — the stand-in for a future `corvus-be`.
//!
//! [`dispatch`] is the `LoopbackBroker` handler: it decodes a JSON param blob,
//! looks the `"<domain>.<verb>"` method up in the `arbor-rpc` registry, and
//! encodes the result back to JSON. There is **no per-command `match` and no
//! arg-struct**: each handler is a plain function annotated with
//! `#[arbor_rpc::handler("…")]`, which reads its signature and self-registers
//! via `inventory` (see [`stash`]).
//!
//! Handlers run the same logic the inline Tauri commands used to, against the
//! live [`AppState`] reached through the captured `AppHandle` (passed in
//! type-erased as `&dyn Any` and downcast back inside each handler). In-process
//! this is a plain call; once `corvus-be` splits out the handlers move into that
//! binary unchanged.

pub mod stash;

// Re-export so backend handlers annotate with `#[corvus::handler]` — the
// product's own namespace for the generic `arbor-rpc` attribute.
pub use arbor_rpc::handler;

use std::any::Any;
use std::collections::HashMap;
use std::sync::OnceLock;

use arbor_ipc::prelude::{Bytes, IpcError};
use arbor_rpc::CallFn;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// The `corvus` handler registry, collected once from every `#[handler]` in
/// this backend's modules.
fn registry() -> &'static HashMap<&'static str, CallFn> {
    static REG: OnceLock<HashMap<&'static str, CallFn>> = OnceLock::new();
    REG.get_or_init(arbor_rpc::registry)
}

/// Decode the JSON params, look the method up, run it against `AppState`,
/// encode the result. Unknown methods surface as [`IpcError::UnknownMethod`];
/// handler failures as [`IpcError::Backend`] (the wire string preserved).
pub fn dispatch(app: &AppHandle, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
    let state = app.state::<AppState>();

    let value: serde_json::Value = if params.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&params).map_err(|e| IpcError::Codec(e.to_string()))?
    };

    let call = registry()
        .get(method)
        .ok_or_else(|| IpcError::UnknownMethod(method.to_string()))?;

    let ctx: &(dyn Any + 'static) = &*state;
    let result = call(ctx, value).map_err(IpcError::Backend)?;
    serde_json::to_vec(&result).map_err(|e| IpcError::Codec(e.to_string()))
}
