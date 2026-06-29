//! [`MerulaState`] — the headless merula backend's owned state.
//!
//! Mirrors `corvus-be`'s `CorvusState` in spirit but for the audio/live-coding
//! product: it holds the event egress, the (lazily-started) audio session, the
//! last good evaluation (replayed on play / queried by scenes), and the reverse
//! channel back to the shell (for jobs). It is **deliberately small and stable** —
//! every field a later wave needs already lives here, so a wave fills in handlers
//! against these accessors and never has to re-edit this file.
//!
//! Unlike `CorvusState` there is no hook broker, no tab→repo registry, and no
//! pushed-config bag: merula-be has no plugin host and resolves its own
//! `merula_config_dir()` / `merula_data_dir()` once `init_active_profile()` ran.

use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::Value;

use crate::session::Session;

/// The state every merula-be handler gets, `Arc`-shared across the dispatcher and
/// any background workers (render jobs, off-thread sample decode).
pub struct MerulaState {
    /// Backend → frontend event egress. Wraps the `arbor-ipc` event channel; the
    /// shell re-emits each topic to the merula window. Call sites use
    /// [`emit`](Self::emit) / [`event_sink`](Self::event_sink).
    sink: Arc<dyn EventSink>,
    /// The live audio session (`None` until the first play, after `Shutdown`, or
    /// after the audio thread exited). Started lazily by the transport handler;
    /// holds the audio-thread `JoinHandle` + the control `Sender` (see
    /// [`Session`](crate::session::Session)).
    session: Mutex<Option<Session>>,
    /// The most recent good evaluation, stashed so a play replays it and the clip
    /// launcher / scene query can read it without re-evaluating. Held as raw JSON
    /// for now — W1/W3 refine it into the typed `Latest` (tracks + cps + tempo +
    /// scenes) once the eval/query domains land.
    latest: Mutex<Option<Value>>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`), set from the
    /// `App`'s host caller. Used by the job-driving domains (render, pack download)
    /// to mint + drive jobs in the shell's single-source `JobRegistry`. `None` only
    /// in the (unused) in-process construction path.
    host: Option<Arc<dyn HostCaller>>,
}

impl MerulaState {
    /// Build the backend state from its event egress. Wave-friendly: a new piece
    /// gains a `with_*` builder (like [`with_host_caller`](Self::with_host_caller))
    /// rather than a new constructor.
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self {
            sink,
            session: Mutex::new(None),
            latest: Mutex::new(None),
            host: None,
        }
    }

    /// Attach the reverse channel back to the shell (the `App`'s host caller).
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Emit a frontend event (mirrors `CorvusState::emit`). The shell re-emits the
    /// topic to the merula window.
    // TODO(clippy): dead_code — deliberate state API mirroring corvus-be; domain
    // handlers will route their events through this once wired. Flagged, not deleted.
    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for a background thread (the audio
    /// thread, a render job) that emits from inside and outlives the borrow of
    /// `&self`.
    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    /// The live audio session slot, for handlers that ensure / send / tear it down.
    /// Returns the guard so the caller drives the lazy-start (transport) or the
    /// send-if-live (mixer overrides) inline — the session is not `Send`-cloned.
    pub fn session(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The last-good-evaluation slot, for the eval domain to stash into and the
    /// transport / scene domains to read from.
    pub fn latest(&self) -> std::sync::MutexGuard<'_, Option<Value>> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Call back into the shell (e.g. the job registry), blocking on the reply.
    /// Errors with a clear message when no reverse channel is wired.
    // TODO(clippy): dead_code — deliberate state API mirroring corvus-be; handlers
    // call back into the shell through this once wired. Flagged, not deleted.
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    /// A cloneable handle to the reverse channel, for a background job worker (a
    /// render / download thread) that drives the shell's `JobRegistry` past the
    /// borrow of `&self`. `None` in the (unused) in-process path.
    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
    }
}
