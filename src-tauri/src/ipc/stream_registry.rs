//! `StreamRegistry` — the cancel side of the streaming seam
//! (`docs/streaming-seam.md`).
//!
//! Maps a `stream_id` to a shared cancel token. A streaming producer registers
//! its id (or an existing token), polls the token while it streams, and removes
//! the entry when the stream ends. The generic `cancel_stream` handler flips the
//! token for any in-flight stream — so one identity (the stream id, == job id
//! where a job exists) addresses the stream's events AND its cancellation,
//! regardless of which backend produced it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Process-wide registry of in-flight cancellable streams. Held by `AppState`
/// as an `Arc` so a producer's spawned task can clone it to remove its entry on
/// completion.
#[derive(Default)]
pub struct StreamRegistry {
    inner: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl StreamRegistry {
    /// Register `id` with the given cancel token (typically shared with the
    /// producer, which polls `token.load(Ordering::Relaxed)`).
    pub fn insert(&self, id: &str, token: Arc<AtomicBool>) {
        if let Ok(mut m) = self.inner.lock() {
            m.insert(id.to_string(), token);
        }
    }

    /// Register `id` and return a fresh cancel token for the producer to poll.
    pub fn register(&self, id: &str) -> Arc<AtomicBool> {
        let token = Arc::new(AtomicBool::new(false));
        self.insert(id, token.clone());
        token
    }

    /// Signal cancellation for `id` — no-op if the id is unknown or the stream
    /// already finished.
    pub fn cancel(&self, id: &str) {
        if let Ok(m) = self.inner.lock() {
            if let Some(t) = m.get(id) {
                t.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Drop `id`'s entry once the stream ends (success, error, or cancellation).
    pub fn remove(&self, id: &str) {
        if let Ok(mut m) = self.inner.lock() {
            m.remove(id);
        }
    }
}
