//! [`CorvusState`] — the headless backend's owned state.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::{EventSink, HostCaller};
use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
use serde_json::{Map, Value};

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
    /// Runtime hook broker, so a handler fires its plugin hooks where it runs
    /// (relocation Wave 0). In-process the shell shares its own dispatcher here
    /// ([`with_hooks`](Self::with_hooks)), so a fire from a `&CorvusState`
    /// handler and a `&AppState` handler hit the same host. In `corvus-be` the
    /// process owns its host and wires a dispatcher bound to it. The default is
    /// an empty dispatcher (no listener) → fires are clean no-ops, which keeps
    /// this crate depending only on the Tauri-free `arbor-plugin-api`.
    hooks: Arc<HookDispatcher>,
    /// App-config slices the shell pushes in, keyed by section name (`"recovery"`,
    /// later `"diff"`, `"gitflow"`, …). Held as raw JSON on purpose: the typed
    /// config structs live in `corvus-git` / the shell (they pull in `git2`),
    /// and this crate stays config-type-free — the consuming handler in
    /// `corvus-be` deserializes its own section into the real type (falling back
    /// to that type's `Default` when a section is absent). In-process the shell
    /// reads its `AppState` config directly and never touches this; it matters
    /// only on the OOP path, where there is no app config in this process.
    config: Mutex<Map<String, Value>>,
    /// Per-tab memoised repository statistics: `tab_id → (cache_key, RepoStats-as-JSON)`.
    /// Held as **JSON**, not the typed `RepoStats` (which lives in the git2-coupled
    /// `corvus-git` crate), so this crate stays git2-free — the `stats` handler
    /// serializes in/out, exactly like the config-bag above. `cache_key` fingerprints
    /// HEAD + the stats-exclude config, so a HEAD move or an exclusion change
    /// invalidates the entry. `Arc` so a background compute thread can hold the cache
    /// past the handler's return (mirrors the shell's `AppState::stats_cache`). Used
    /// only on the OOP path; in-process the shell keeps its own typed cache.
    stats_cache: Arc<Mutex<HashMap<String, (String, Value)>>>,
    /// Tabs with an in-flight stats computation — the dedup guard that makes a second
    /// `compute_repo_stats` while one is already running a no-op (last finisher wins).
    stats_computing: Arc<Mutex<HashSet<String>>>,
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
            hooks: Arc::new(HookDispatcher::new()),
            config: Mutex::new(Map::new()),
            stats_cache: Arc::new(Mutex::new(HashMap::new())),
            stats_computing: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Attach the reverse channel back to the shell. Called by `corvus-be` once
    /// it owns a [`FrameHostCaller`](arbor_ipc::prelude::FrameHostCaller); the
    /// in-process shell leaves it unset.
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Attach the hook broker. In-process the shell passes a clone of its own
    /// `Arc<HookDispatcher>` (so both states fire onto the same host); in
    /// `corvus-be` the process builds one bound to its local plugin host.
    pub fn with_hooks(mut self, hooks: Arc<HookDispatcher>) -> Self {
        self.hooks = hooks;
        self
    }

    /// Fire a fire-and-forget plugin hook to every subscriber, synchronously.
    /// Thin bridge over the dispatcher (`serde_json::Value` → `PluginValue`),
    /// mirroring the shell's `AppState::fire_hook` so handlers read the same.
    /// A no-op when no listener is wired (the default empty dispatcher).
    pub fn fire_hook(&self, hook: &str, ctx: Value) {
        self.hooks.fire_blocking(hook, PluginValue::from_json(ctx));
    }

    /// Fire the vetoable `on_pre_commit` hook; `Some(reason)` aborts the commit
    /// (the reason is surfaced to the user). Runs entirely inside this process's
    /// host — no cross-process round-trip — so a co-located commit handler keeps
    /// the veto's pre-mutation timing.
    pub fn fire_pre_commit_veto(&self, ctx: Value) -> Option<String> {
        self.hooks
            .fire_vetoable_blocking("on_pre_commit", PluginValue::from_json(ctx))
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

    /// A cloneable handle to the reverse channel, for a blocking task (e.g. a
    /// `spawn_blocking` running libgit2 whose credential callback must call back
    /// to the shell) that outlives the borrow of `&self`. `None` in-process.
    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
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

    /// Push (or replace) a config section, keyed by name. The shell sends the
    /// serialized app-config slice the OOP handlers need (e.g. `"recovery"`);
    /// the handler deserializes it back into its own typed config. Generic by
    /// design: a new config-dependent domain adds a section here, not a field.
    pub fn set_config(&self, section: &str, value: Value) {
        if let Ok(mut c) = self.config.lock() {
            c.insert(section.to_string(), value);
        }
    }

    /// Read a previously-pushed config section, or `None` if the shell hasn't
    /// pushed it (the handler then falls back to its type's `Default`).
    pub fn config(&self, section: &str) -> Option<Value> {
        self.config.lock().ok().and_then(|c| c.get(section).cloned())
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

    /// The memoised-stats cache handle (a cheap `Arc` clone), for a background
    /// compute thread that outlives the borrow of `&self`. Keyed
    /// `tab_id → (cache_key, RepoStats-as-JSON)`; the handler owns the typed
    /// `RepoStats` and serializes it in / out (this crate stays git2-free).
    pub fn stats_cache(&self) -> Arc<Mutex<HashMap<String, (String, Value)>>> {
        Arc::clone(&self.stats_cache)
    }

    /// The in-flight-stats dedup-guard handle (a cheap `Arc` clone) — the set of
    /// tabs currently computing, so a duplicate `compute_repo_stats` is a no-op.
    pub fn stats_computing(&self) -> Arc<Mutex<HashSet<String>>> {
        Arc::clone(&self.stats_computing)
    }

    /// A cloneable handle to the hook broker, for a background task (e.g. the
    /// worktree-link checkout-sync orchestrator) that fires fire-and-forget hooks
    /// from a thread outliving the borrow of `&self`. Mirrors [`event_sink`](Self::event_sink).
    pub fn hooks_handle(&self) -> Arc<HookDispatcher> {
        Arc::clone(&self.hooks)
    }

    /// Snapshot of every open tab as `(tab_id, repo_path)` — the shell-pushed
    /// open-repo set. Used by the orchestrator to target `arbor://graph-refresh`
    /// at the tabs showing a synced member.
    pub fn open_tabs(&self) -> Vec<(String, String)> {
        self.repos
            .lock()
            .map(|r| r.iter().map(|(t, p)| (t.clone(), p.clone())).collect())
            .unwrap_or_default()
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

    /// A `HookListener` that records fired names and can pre-set a veto reason —
    /// stands in for the real mlua listener so the seam is tested host-free.
    #[derive(Default)]
    struct RecordingListener {
        fired: Mutex<Vec<String>>,
        veto:  Option<String>,
    }
    #[async_trait::async_trait]
    impl arbor_plugin_api::prelude::HookListener for RecordingListener {
        async fn fire(&self, name: &str, _ctx: &PluginValue) {
            self.fired.lock().unwrap().push(name.to_string());
        }
        async fn fire_vetoable(&self, _name: &str, _ctx: &PluginValue) -> Option<String> {
            self.veto.clone()
        }
    }

    fn dispatcher_with(listener: Arc<dyn arbor_plugin_api::prelude::HookListener>) -> Arc<HookDispatcher> {
        let mut d = HookDispatcher::new();
        d.register_listener(listener);
        Arc::new(d)
    }

    #[test]
    fn fire_hook_reaches_the_listener() {
        let rec = Arc::new(RecordingListener::default());
        let state = CorvusState::new(Arc::new(RecordingSink::default()))
            .with_hooks(dispatcher_with(rec.clone()));
        state.fire_hook("on_stash_push", json!({ "index": 0 }));
        assert_eq!(rec.fired.lock().unwrap().as_slice(), &["on_stash_push".to_string()]);
    }

    #[test]
    fn fire_hook_is_a_noop_without_a_listener() {
        // Default dispatcher (no listener) → the fire must not panic.
        let state = CorvusState::new(Arc::new(RecordingSink::default()));
        state.fire_hook("on_stash_push", json!({}));
    }

    #[test]
    fn config_section_round_trips_and_defaults_to_none() {
        let state = CorvusState::new(Arc::new(RecordingSink::default()));
        // Absent section → None (handler falls back to its Default).
        assert_eq!(state.config("recovery"), None);
        // Push then read back the exact JSON the shell would send.
        state.set_config("recovery", json!({ "retention_days": 7 }));
        assert_eq!(state.config("recovery"), Some(json!({ "retention_days": 7 })));
        // A second push replaces the section wholesale.
        state.set_config("recovery", json!({ "retention_days": 30 }));
        assert_eq!(state.config("recovery"), Some(json!({ "retention_days": 30 })));
        // Sections are independent.
        assert_eq!(state.config("diff"), None);
    }

    #[test]
    fn pre_commit_veto_propagates() {
        let rec = Arc::new(RecordingListener { veto: Some("nope".to_string()), ..Default::default() });
        let state = CorvusState::new(Arc::new(RecordingSink::default()))
            .with_hooks(dispatcher_with(rec));
        assert_eq!(state.fire_pre_commit_veto(json!({})), Some("nope".to_string()));
    }
}
