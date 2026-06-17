//! [`CorvusState`] — the headless backend's owned state.

use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use serde_json::Value;

/// The state the Corvus (git) backend owns — the seed of `corvus-be`.
///
/// In-process today the shell constructs one and routes its `AppState`'s event
/// egress through it. It deliberately holds **only transport-ready pieces**: the
/// event sink for now, then the git registries (`RepoManager`, `JobRegistry`, …)
/// as those are extracted from the shell. A handler reached through the IPC seam
/// will eventually take `&CorvusState` instead of the shell's `AppState`; its git
/// logic doesn't change, only the state type it is handed.
pub struct CorvusState {
    /// Backend → frontend event egress. In-process the shell backs this with
    /// `AppHandle::emit`; once `corvus-be` splits out it wraps the `arbor-ipc`
    /// event channel — call sites ([`emit`](Self::emit)) don't change.
    events: Arc<dyn EventSink>,
}

impl CorvusState {
    /// Build the backend state from its event egress. As more pieces move in,
    /// this gains parameters (the git registries) rather than new constructors.
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self { events }
    }

    /// Emit a frontend event. Model-D-safe: in-process it forwards to
    /// `AppHandle::emit`; post-split it becomes an `arbor-ipc` `Event::Notify`.
    pub fn emit(&self, topic: &str, payload: Value) {
        self.events.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for background threads/tasks that
    /// outlive a call and emit from inside — they capture this (`Send + 'static`)
    /// instead of an `AppHandle`.
    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.events)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;

    /// Records every emitted (topic, payload) so the delegation can be asserted.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<(String, Value)>>,
    }
    impl EventSink for RecordingSink {
        fn emit(&self, topic: &str, payload: Value) {
            self.events.lock().unwrap().push((topic.to_string(), payload));
        }
    }

    #[test]
    fn emit_forwards_to_the_sink() {
        let sink = Arc::new(RecordingSink::default());
        let state = CorvusState::new(sink.clone());
        state.emit("arbor://thing", json!({ "n": 1 }));
        let recorded = sink.events.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "arbor://thing");
        assert_eq!(recorded[0].1, json!({ "n": 1 }));
    }

    #[test]
    fn event_sink_shares_the_same_egress() {
        let sink = Arc::new(RecordingSink::default());
        let state = CorvusState::new(sink.clone());
        // A background-thread handle emits onto the very same sink.
        state.event_sink().emit("arbor://bg", Value::Null);
        assert_eq!(sink.events.lock().unwrap()[0].0, "arbor://bg");
    }
}
