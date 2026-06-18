//! In-process `platform` backend — the stand-in for a future `platform-be`.
//!
//! Mirror of [`crate::ipc::corvus`], but for the **platform** product: the
//! app-agnostic services that aren't a git/CI/issue product — config, theme,
//! session, workspace, jobs, fs, terminal, app metadata. These belong in their
//! own backend namespace (never the `corvus` one) so that when the products
//! split out of the shell, config/fs/workspace travel with `platform-be`, not
//! `corvus-be`.
//!
//! Handlers are plain functions annotated `#[platform::handler(program =
//! "platform")]`. The `program = "platform"` tag is what keeps them in this
//! backend's slice of the shared `arbor-rpc` inventory (see [`dispatch`]);
//! every handler in this module tree must carry it. Like the corvus handlers
//! they run against the live [`AppState`] reached through the captured
//! `AppHandle`; in-process this is a plain call, and once `platform-be` splits
//! out they move into that binary unchanged.

pub mod app;
pub mod branding;
pub mod cloud;
pub mod config;
pub mod deep_link;
pub mod fs;
pub mod jobs;
pub mod marketplace;
pub mod plugin;
pub mod plugin_logs;
pub mod plugin_templates;
pub mod post_hooks;
pub mod scheduler;
pub mod session;
pub mod stream;
pub mod terminal;
pub mod theme;
pub mod workspace;
pub mod workspace_runs;

// Re-export so backend handlers annotate with `#[platform::handler(...)]` — the
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
/// `platform(...)` helper.
pub const PROGRAM: &str = "platform";

/// The `platform` handler registry, collected once from every
/// `#[handler(program = "platform")]` in this backend's modules. Filtered by
/// program so the shell's `corvus` handlers (which share this binary's
/// inventory) never leak into the platform dispatch.
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
