//! In-process `studio` backend — the stand-in for a future `studio-be`.
//!
//! Mirror of [`crate::ipc::corvus`] / [`crate::ipc::platform`], but for the
//! **studio** product: the standalone CI/pipeline-config editor (YAML convert/
//! format/validate, schema reflection). It has essentially zero coupling to the
//! git or platform backends, which is why it is the cleanest separate product —
//! these handlers move into a `studio-be` binary unchanged once it splits out.
//!
//! Handlers are plain functions annotated `#[studio::handler(program =
//! "studio")]`. The `program = "studio"` tag keeps them in this backend's slice
//! of the shared `arbor-rpc` inventory (see [`dispatch`]); every handler in this
//! module tree must carry it. In-process they run against the live [`AppState`]
//! reached through the captured `AppHandle`; once `studio-be` splits out they
//! move into that binary unchanged.

pub mod config;
pub mod index;

// Re-export so backend handlers annotate with `#[studio::handler(...)]` — the
// product's own namespace for the generic `arbor-rpc` attribute.
pub use arbor_rpc::handler;

use std::any::Any;
use std::collections::HashMap;
use std::sync::OnceLock;

use arbor_ipc::prelude::{Bytes, IpcError};
use arbor_rpc::CallFn;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// This backend's program label — the `program = …` every handler here tags
/// itself with, and the router product name the FE addresses via the
/// `studio(...)` helper.
pub const PROGRAM: &str = "studio";

/// The `studio` handler registry, collected once from every
/// `#[handler(program = "studio")]` in this backend's modules. Filtered by
/// program so the shell's other backends' handlers (which share this binary's
/// inventory) never leak into the studio dispatch.
fn registry() -> &'static HashMap<&'static str, CallFn> {
    static REG: OnceLock<HashMap<&'static str, CallFn>> = OnceLock::new();
    REG.get_or_init(|| arbor_rpc::registry_for(PROGRAM))
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
