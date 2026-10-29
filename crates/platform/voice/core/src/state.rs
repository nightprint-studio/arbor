//! [`VoiceState`] — what every `voice-be` handler gets: event egress, the live config and the
//! activity clock the idle watchdog reads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use arbor_ipc::prelude::EventSink;
use serde_json::Value;

use crate::config::VoiceConfig;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub struct VoiceState {
    sink: Arc<dyn EventSink>,
    config: RwLock<VoiceConfig>,
    last_activity_ms: AtomicU64,
}

impl VoiceState {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink, config: RwLock::new(VoiceConfig::load()), last_activity_ms: AtomicU64::new(now_ms()) }
    }

    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    pub fn config(&self) -> VoiceConfig {
        self.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    /// Replace the live config and persist it.
    pub fn set_config(&self, cfg: VoiceConfig) -> Result<VoiceConfig, String> {
        let cfg = cfg.sanitized();
        cfg.save()?;
        if let Ok(mut w) = self.config.write() {
            *w = cfg.clone();
        }
        Ok(cfg)
    }

    /// Apply a partial change (`{"language": "en"}`) to the live config and persist it. The merge
    /// happens under the write lock, so two writers (the page and the backend's own model
    /// selection) can never overwrite each other's fields with a stale copy.
    pub fn patch_config(&self, patch: &Value) -> Result<VoiceConfig, String> {
        let mut guard = self.config.write().map_err(|_| "the voice settings are unavailable".to_string())?;
        let next = guard.patched(patch)?;
        next.save()?;
        *guard = next.clone();
        Ok(next)
    }

    /// Mark the process as used (resets the idle clock).
    pub fn touch(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Milliseconds since the last [`touch`](Self::touch).
    pub fn idle_ms(&self) -> u64 {
        now_ms().saturating_sub(self.last_activity_ms.load(Ordering::Relaxed))
    }
}
