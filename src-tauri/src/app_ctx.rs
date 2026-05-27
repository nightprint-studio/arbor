//! `AppCtx` bridge between Tauri's `AppHandle` and the Tauri-agnostic
//! domain crates (`arbor-scheduler`, future `arbor-plugin-*`, …).
//!
//! Domain crates that need to emit events, locate the Arbor data root, or
//! read the user-focus signal take a `&dyn AppCtx` instead of a Tauri
//! handle. This module implements that trait once on top of `AppHandle`
//! + the `AppState.app_focused` flag, and the shell crate hands the
//! resulting trait object to every consumer (scheduler today; more crates
//! as the refactor progresses).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arbor_core::prelude::AppCtx;
use tauri::{AppHandle, Emitter};

pub struct TauriAppCtx {
    handle:    AppHandle,
    focused:   Arc<AtomicBool>,
    arbor_dir: PathBuf,
}

impl TauriAppCtx {
    pub fn new(handle: AppHandle, focused: Arc<AtomicBool>) -> Self {
        Self {
            handle,
            focused,
            arbor_dir: arbor_core::prelude::arbor_config_dir(),
        }
    }
}

impl AppCtx for TauriAppCtx {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.handle.emit(event, payload) {
            tracing::warn!("AppCtx emit '{event}' failed: {e}");
        }
    }

    fn arbor_dir(&self) -> &Path {
        &self.arbor_dir
    }

    fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }
}
