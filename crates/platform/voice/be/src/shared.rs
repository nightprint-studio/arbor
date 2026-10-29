//! The one `Arc<VoiceState>`, reachable from handler code that only receives `&VoiceState`
//! but must hand an owned handle to a background thread.

use std::sync::{Arc, OnceLock};

use voice_core::prelude::VoiceState;

static STATE: OnceLock<Arc<VoiceState>> = OnceLock::new();

pub fn init(state: Arc<VoiceState>) {
    let _ = STATE.set(state);
}

/// Panics only if called before `main` finished wiring, which no handler can do.
pub fn get() -> Arc<VoiceState> {
    Arc::clone(STATE.get().expect("voice state not initialised"))
}
