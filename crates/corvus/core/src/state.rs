//! [`CorvusState`] — the headless backend's owned state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::{EventSink, HostCaller};
use serde_json::Value;

/// The state the Corvus (git) backend owns — the seed of `corvus-be`.
///
/// In-process today the shell constructs one and routes its `AppState`'s event
/// egress through it. It deliberately holds **only transport-ready pieces**: the
/// event sink + a tab→repo registry for now, then the git registries
/// (`JobRegistry`, …) as those are extracted from the shell. A handler reached
/// through the IPC seam takes `&CorvusState` instead of the shell's `AppState`;
/// its git logic doesn't change, only the state type it is handed.
pub struct CorvusState {
    /// Backend → frontend event egress. In-process the shell backs this with
    /// `AppHandle::emit`; once `corvus-be` splits out it wraps the `arbor-ipc`
    /// event channel — call sites ([`emit`](Self::emit)) don't change.
    events: Arc<dyn EventSink>,
    /// `tab_id` → repo path. The headless process has no `RepoManager`, so the
    /// shell pushes the open repos here (on repo open/close) and handlers resolve
    /// a tab to its path through [`repo_path`](Self::repo_path).
    repos: Mutex<HashMap<String, String>>,
    /// The git program the shell resolved (PATH / configured / portable). `None`
    /// → fall back to `git` on `PATH`. Pushed by the shell so the backend shells
    /// out to the same binary.
    git_program: Mutex<Option<String>>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`): present
    /// only when split into its own process (`corvus-be` wires a
    /// `FrameHostCaller`). In-process it's `None` — those handlers reach the
    /// shell's vault / plugin host directly and never call back.
    host: Option<Arc<dyn HostCaller>>,
}

impl CorvusState {
    /// Build the backend state from its event egress. As more pieces move in,
    /// this gains parameters (the git registries) rather than new constructors.
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self {
            events,
            repos: Mutex::new(HashMap::new()),
            git_program: Mutex::new(None),
            host: None,
        }
    }

    /// Attach the reverse channel back to the shell. Called by `corvus-be` once
    /// it owns a [`FrameHostCaller`](arbor_ipc::prelude::FrameHostCaller); the
    /// in-process shell leaves it unset.
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Call back into the shell (credential resolution, plugin-UI round-trips),
    /// blocking on the reply. Errors with a clear message when no reverse channel
    /// is wired (in-process, where it shouldn't be reached).
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    /// Register (or update) a tab's repo path. Pushed by the shell on repo open.
    pub fn register_repo(&self, tab_id: String, path: String) {
        if let Ok(mut repos) = self.repos.lock() {
            repos.insert(tab_id, path);
        }
    }

    /// Forget a tab's repo. Pushed by the shell on repo close.
    pub fn deregister_repo(&self, tab_id: &str) {
        if let Ok(mut repos) = self.repos.lock() {
            repos.remove(tab_id);
        }
    }

    /// Resolve a tab to its repo path, or `None` if the shell hasn't registered
    /// it (a handler should surface a clear error in that case).
    pub fn repo_path(&self, tab_id: &str) -> Option<String> {
        self.repos.lock().ok().and_then(|r| r.get(tab_id).cloned())
    }

    /// Set the git program the backend should shell out to (pushed by the shell).
    pub fn set_git_program(&self, program: Option<String>) {
        if let Ok(mut g) = self.git_program.lock() {
            *g = program;
        }
    }

    /// The git program to shell out to, or `None` → `git` on `PATH`.
    pub fn git_program(&self) -> Option<String> {
        self.git_program.lock().ok().and_then(|g| g.clone())
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
