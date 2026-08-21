//! Cloud-storage domain — the cancellation flags, and nothing else.
//!
//! ## What used to be here
//!
//! Twenty handlers: secrets, listings, stat/delete/copy, the transfers, the streaming
//! listings, the OAuth start. They date from when the cloud panel was built into the
//! frontend and called the shell directly.
//!
//! It is not built in any more — the panel is the `cloud` plugin, and its calls go
//! `arbor.cloud.*` → the corvus plugin namespace → the `__cloud_*` methods on the shell's
//! reverse channel. So these had no callers left, and that is worse than dead code: they were
//! a **second implementation of the same operations**, reached by a different name, which the
//! wasm provider routing in [`crate::cloud_guest`] does not touch. A listing that came back
//! through here would silently bypass an installed provider, and nothing would say so.
//!
//! What survives is the pair the download-progress modal actually calls. They belong to the
//! frontend rather than to the plugin: the modal owns a Cancel button, and cancellation is a
//! flag in `AppState`, not an operation on a bucket.

use crate::error::{AppError, Result};
use crate::ipc::platform;
use crate::AppState;

// ── Cancellation ──────────────────────────────────────────────────────────

#[platform::handler(program = "platform")]
fn cloud_cancel(state: &AppState, stream_id: String) -> Result<()> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    if let Some(flag) = map.get(&stream_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[platform::handler(program = "platform")]
fn cloud_is_cancelled(state: &AppState, stream_id: String) -> Result<bool> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    Ok(map.get(&stream_id)
        .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false))
}
