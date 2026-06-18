//! Generic streaming-seam control handlers (`docs/streaming-seam.md`).
//!
//! Registered under `platform` because the [`StreamRegistry`](crate::ipc::stream_registry::StreamRegistry)
//! is global on `AppState` — one `cancel_stream` handler cancels any in-flight
//! stream by its id, whatever backend produced it. The FE
//! `startStream(...).cancel()` calls this.

use crate::error::Result;
use crate::ipc::platform;
use crate::AppState;

/// Cancel an in-flight stream by its id. No-op if the id is unknown or the
/// stream already finished — cancellation is best-effort by design.
#[platform::handler(program = "platform")]
fn cancel_stream(state: &AppState, stream_id: String) -> Result<()> {
    state.streams.cancel(&stream_id);
    Ok(())
}
