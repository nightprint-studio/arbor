//! Application state shared across every Tauri command.
//!
//! [`AppState`] is the shell's process-wide state bag (managed by Tauri,
//! reached via `State<'_, AppState>` in commands or `app.state::<AppState>()`
//! in setup). It owns the in-process registries (plugins, jobs, pipelines,
//! workspaces, providers, …) plus the seams to the out-of-process products —
//! most notably the [`CorvusState`] event egress. The open-tab → repo registry
//! is **not** here: `corvus-be` owns it (the launcher keeps no `git2`/`RepoManager`).
//!
//! The hook-dispatcher builder lives in `corvus_plugin::prelude::build_hook_dispatcher`
//! — one definition the shell (in-process host) and `corvus-be` (OOP host) both
//! build through, so a fire fans out identically wherever the handler runs.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

use arbor_cloud::host::CloudHost;
use arbor_ipc::prelude::EventSink;
use arbor_plugin_api::prelude::HookDispatcher;
use arbor_plugin_core::prelude::{PluginHost, ToolchainRegistry};
use arbor_plugin_marketplace::prelude::MarketplaceRegistry;
use arbor_scheduler::prelude::Scheduler;
use arbor_shell_common::prelude::Router;
use corvus_brp::prelude::BrpRegistry;
use corvus_core::prelude::CorvusState;

use crate::branding::BrandingState;
use crate::cloud::{CloudCancellations, CloudPendingOps};
use crate::config::app_config::AppConfig;
use crate::deep_link::DeepLinkBuffer;
use crate::error::{AppError, Result};
use crate::git_provider::{GitProviderRegistry, GithubProvider, GitlabProvider};
use crate::jobs::JobRegistry;
use crate::pipeline::PipelineRegistry;
use crate::plugin_logs::PluginLogBuffer;
use crate::studio::format::StudioRegistry;
use crate::terminal::TerminalManager;
use crate::workspace::{RepoRegistry, WorkspaceStore};

// ---------------------------------------------------------------------------
// Application state — shared across all Tauri commands
// ---------------------------------------------------------------------------

pub struct AppState {
    /// Arc-wrapped because the `arbor-cloud` CloudHost impl holds a clone —
    /// both AppState's `lock_plugin_host()` helper and the cloud crate's
    /// `host.fire_plugin_hook()` need access. Arc<Mutex<—>> keeps both
    /// pointing at the same lock without ownership tricks.
    pub plugin_host:    Arc<Mutex<PluginHost>>,
    /// Runtime-agnostic hook broker (PR #4 — `arbor-plugin-core`). Built once
    /// in `new()` with the static `HOOK_CATALOG` registered and a single
    /// `LuaHookListener` bound to `plugin_host`. Domain code fires hooks
    /// through this (`fire_blocking` / `fire_vetoable_blocking` from sync
    /// commands, `.fire(...).await` from async) instead of reaching into
    /// `PluginHost` directly. Arc so background threads can clone + fire after
    /// a command returns.
    pub hook_dispatcher: Arc<HookDispatcher>,
    pub config:         Mutex<AppConfig>,
    pub terminals:      Mutex<TerminalManager>,
    /// Arc-wrapped for the same reason as `plugin_host` — the cloud
    /// crate's CloudHost impl needs to register/append/set status on jobs
    /// from its spawned tokio tasks.
    pub jobs:           Arc<Mutex<JobRegistry>>,
    /// Ring-buffer of recent `arbor.log.*` entries from every plugin —
    /// powers the Plugin Logs bottom panel. Arc-wrapped so the pipeline
    /// orchestrator's injected `PipelineRuntime` shares the same buffer
    /// without reaching back through an `AppHandle`.
    pub plugin_logs:    Arc<Mutex<PluginLogBuffer>>,
    /// Self-contained pipeline-engine state (run/def registry + the
    /// concurrency condvar). Lifted out of `AppState` as an `Arc` so the
    /// orchestrator worker thread shares it via the injected `PipelineRuntime`
    /// instead of reaching into `AppState` / `AppHandle`.
    pub pipeline_engine: Arc<crate::pipeline::PipelineEngine>,
    /// True when the app window has focus; used by focus-gated schedulers.
    pub app_focused:    Arc<AtomicBool>,
    /// The currently active tab ID as reported by the frontend.
    pub active_tab_id:  Arc<Mutex<Option<String>>>,
    /// The active tab's repo path, cached on `set_active_tab`. The launcher keeps
    /// no repo registry — `corvus-be` owns it — but the plugin host
    /// (`arbor.settings.project`) and the reverse-channel `__pipeline_run` cwd
    /// fallback need the active repo path *without* a re-entrant call back into
    /// `corvus-be` (which, from inside a host-dispatch reverse call, could
    /// deadlock). So we cache just this one path on every tab switch.
    pub active_repo_path: Arc<Mutex<Option<String>>>,
    /// Installed toolchain registry (toolchains/<kind>.json).
    pub toolchain_registry: Arc<Mutex<ToolchainRegistry>>,
    /// Central registry of every repo Arbor has ever been shown.
    /// Referenced by workspaces by UUID — path edits flow from here.
    pub repo_registry: Mutex<RepoRegistry>,
    /// List of user-defined workspaces (plus the implicit Scratch one) and
    /// currently-active workspace id.  Tab snapshots live in separate files.
    pub workspaces:    Mutex<WorkspaceStore>,
    /// Unified registry of remote git host clients (GitHub / GitLab / —).
    /// Populated at boot — see `git_provider/`.
    pub git_providers: Mutex<GitProviderRegistry>,
    /// In-memory branding overrides applied by plugins (logo, etc.).
    pub branding: BrandingState,
    /// Cold-start buffer for `arbor://—` URLs received before the frontend
    /// has signalled readiness via the `deep_link_ready` IPC.
    pub deep_link_buffer: Arc<DeepLinkBuffer>,
    /// Unified per-format backend registry (RON / JSON / TOML / YAML /
    /// `.properties`). Each backend owns its own document state behind
    /// its own interior Mutex; this registry is immutable after init —
    /// see `studio/format/registry.rs` + FROZEN F17 in
    /// `project_studio_multi_format.md`. JSON state lives inside
    /// `JsonBackend` since Phase 3.a — no separate AppState field.
    pub studio_registry: Arc<StudioRegistry>,
    /// Per-job cancellation flags for cloud-storage transfer tasks (which
    /// run as in-process tokio tasks, not subprocesses — so the standard
    /// PID-kill cancel path doesn't apply). `cancel_job` flips the right
    /// flag before falling through. Earmarked to be deleted alongside the
    /// rest of the cloud-storage host code when WASM lands.
    pub cloud_cancellations: Arc<CloudCancellations>,
    /// Generic streaming-seam cancellation registry (stream_id → cancel token),
    /// shared via `Arc` so a producer's spawned task can remove its entry on
    /// completion. The generic `cancel_stream` handler flips a token here.
    pub streams: Arc<crate::ipc::stream_registry::StreamRegistry>,
    /// stream_id — JobRegistry job_id for `download_many` calls with
    /// `keep_open=true` (chunk-merge flow). `cloud_report_done` reads +
    /// removes the entry to finalize the job once the merge phase ends.
    pub cloud_pending_ops: Arc<CloudPendingOps>,
    /// The cloud host singleton, published by `cloud::install()`. The single
    /// home of the `Arc<dyn CloudHost>`: both the platform command handlers and
    /// the Lua `ns_shell/cloud.rs` path reach it via `cloud_host()` — no
    /// Tauri-managed state is involved.
    pub cloud_host: Arc<OnceLock<Arc<dyn CloudHost>>>,
    /// Bevy Remote Protocol — singleton live session against one Bevy game
    /// at a time. Read-only HTTP for Phase 1; SSE watch + editing in later
    /// phases. See `project_bevy_brp_client.md` memory.
    pub brp: Mutex<BrpRegistry>,
    /// Plugin & theme marketplace registry — lives in
    /// `arbor-plugin-marketplace`, wired to the shell via
    /// `TauriMarketplaceHost`.
    pub marketplace: Mutex<MarketplaceRegistry>,
    /// Mirrors the `arbor://boot-progress` / `arbor://boot-done` event stream
    /// in shared state as a safety net for dev-mode HMR remounts where the
    /// listener attaches after the events have already fired (the
    /// `frontend_ready` handshake doesn't help there because the boot thread
    /// already passed its gate on first launch). `BootSplash.svelte` polls
    /// `get_boot_state` on mount and dismisses immediately when `done == true`.
    pub boot_done:     Arc<AtomicBool>,
    pub boot_progress: Arc<Mutex<Option<serde_json::Value>>>,
    /// Handshake: BootSplash calls `frontend_ready` after registering its
    /// `arbor://boot-progress` / `arbor://boot-done` listeners. The boot
    /// thread waits on this condvar (with a safety timeout) before emitting
    /// progress events, so events never land before listeners exist. The
    /// fallback timeout means a hung / missing frontend can't strand boot.
    pub frontend_ready: Arc<(Mutex<bool>, Condvar)>,
    /// Shared trigger engine — drives both the marketplace auto-refresh
    /// and every plugin-declared `arbor.scheduler.register`. Filled inside
    /// `setup()` once the Tokio runtime handle is reachable (an `OnceLock`
    /// because `AppState::new()` runs before the runtime is available).
    /// Read via [`AppState::scheduler`].
    pub scheduler: Arc<OnceLock<Arc<Scheduler>>>,
    /// Model-D IPC router (M3 Asse B). Maps a FE `invoke` to the right product
    /// backend over `arbor-ipc`. Today it fronts an in-process `LoopbackBroker`
    /// (one process); the same router will front a pipe/socket-backed client
    /// once the backends split out. Filled inside `setup()` (it captures the
    /// `AppHandle` the loopback dispatch needs, which `AppState::new()` predates)
    /// — an `OnceLock` like [`scheduler`](Self::scheduler). Read via
    /// [`AppState::router`].
    pub router: Arc<OnceLock<Arc<Router>>>,
    /// The Corvus (git) backend's headless state — the seed of `corvus-be`
    /// (`corvus-core`). Published in `setup()` (it needs the `AppHandle` to back
    /// its event sink, which `AppState::new()` predates, hence the `OnceLock`).
    /// Today it holds only the event egress: a Model-D handler reached through
    /// the generic `rpc` command holds `&AppState`, and [`emit`](Self::emit) /
    /// [`event_sink`](Self::event_sink) route through here so handlers push
    /// events without taking an `AppHandle`. As git domains are extracted this
    /// gains fields and handlers shift to `&CorvusState`; at the process split
    /// it moves into `corvus-be` and its sink flips to the `arbor-ipc` channel —
    /// the call sites stay put.
    pub corvus: Arc<OnceLock<CorvusState>>,
}

impl AppState {
    // — Mutex lock helpers —
    // Each helper wraps the raw `.lock()` call, logs the poisoning context and
    // converts it to the typed `AppError::MutexPoisoned` variant so callers get
    // a meaningful error message instead of a silent panic.

    pub fn lock_plugin_host(&self) -> Result<MutexGuard<'_, PluginHost>> {
        self.plugin_host.lock().map_err(|e| {
            tracing::error!("plugin_host mutex poisoned: {e}");
            AppError::MutexPoisoned("plugin_host".into())
        })
    }

    /// Fire a hook to every subscribing plugin, synchronously — the common
    /// case for Tauri command threads. Thin wrapper over the hook dispatcher
    /// that bridges `serde_json::Value` — `PluginValue` so call sites stay
    /// terse. Async contexts can call `state.hook_dispatcher.fire(...).await`
    /// directly instead.
    pub fn fire_hook(&self, hook: &str, ctx: serde_json::Value) {
        self.hook_dispatcher
            .fire_blocking(hook, arbor_plugin_api::prelude::PluginValue::from_json(ctx));
    }

    /// Emit a frontend event. The Model-D-safe egress for IPC handlers reached
    /// through the generic `rpc` command (which hold only `&AppState`, not an
    /// `AppHandle`): it routes through [`CorvusState`], whose sink in-process
    /// forwards to `AppHandle::emit` and post-split becomes an `arbor-ipc`
    /// `Event::Notify` the shell re-emits — the call site doesn't change. A drop
    /// before the backend is wired is logged, not panicked (only during early
    /// boot). The payload is serialized to JSON here, exactly as crossing the
    /// IPC boundary will require.
    pub fn emit<S: serde::Serialize>(&self, event: &str, payload: S) {
        let Some(corvus) = self.corvus.get() else {
            tracing::warn!("AppState::emit('{event}') before backend was wired — dropped");
            return;
        };
        match serde_json::to_value(payload) {
            Ok(value) => corvus.emit(event, value),
            Err(e) => tracing::warn!("AppState::emit('{event}') serialize failed: {e}"),
        }
    }

    /// A cloneable handle to the frontend event egress, for background
    /// threads/tasks that outlive a handler and emit from inside — they capture
    /// this (`Arc<dyn EventSink>`, `Send + 'static`) instead of an `AppHandle`,
    /// so they're already shaped for the `corvus-be` split. `None` only before
    /// `setup()` wires the backend.
    pub fn event_sink(&self) -> Option<Arc<dyn EventSink>> {
        self.corvus.get().map(|c| c.event_sink())
    }

    pub fn lock_config(&self) -> Result<MutexGuard<'_, AppConfig>> {
        self.config.lock().map_err(|e| {
            tracing::error!("config mutex poisoned: {e}");
            AppError::MutexPoisoned("config".into())
        })
    }

    pub fn lock_terminals(&self) -> Result<MutexGuard<'_, TerminalManager>> {
        self.terminals.lock().map_err(|e| {
            tracing::error!("terminals mutex poisoned: {e}");
            AppError::MutexPoisoned("terminals".into())
        })
    }

    pub fn lock_jobs(&self) -> Result<MutexGuard<'_, JobRegistry>> {
        self.jobs.lock().map_err(|e| {
            tracing::error!("jobs mutex poisoned: {e}");
            AppError::MutexPoisoned("jobs".into())
        })
    }

    pub fn lock_plugin_logs(&self) -> Result<MutexGuard<'_, PluginLogBuffer>> {
        self.plugin_logs.lock().map_err(|e| {
            tracing::error!("plugin_logs mutex poisoned: {e}");
            AppError::MutexPoisoned("plugin_logs".into())
        })
    }

    pub fn lock_pipelines(&self) -> Result<MutexGuard<'_, PipelineRegistry>> {
        self.pipeline_engine.registry.lock().map_err(|e| {
            tracing::error!("pipelines mutex poisoned: {e}");
            AppError::MutexPoisoned("pipelines".into())
        })
    }

    /// Shared handle to the pipeline engine (registry + concurrency condvar).
    pub fn pipeline_engine(&self) -> Arc<crate::pipeline::PipelineEngine> {
        self.pipeline_engine.clone()
    }

    /// Build the runtime the orchestrator needs from this state. Returns `None`
    /// when the event sink isn't wired yet (only during early boot, before
    /// `setup()`).
    pub fn pipeline_runtime(&self) -> Option<crate::pipeline::PipelineRuntime> {
        let sink = self.event_sink()?;
        // `pipelines` config is owned by corvus-be (`corvus/config.toml`); read
        // the concurrency cap back with a thin partial-struct read. Defaults to 4.
        let max_concurrent_runs = {
            #[derive(serde::Deserialize)]
            struct PipelinesProbe {
                #[serde(default = "default_max_runs")]
                max_concurrent_runs: u32,
            }
            fn default_max_runs() -> u32 { 4 }
            crate::config::corvus_read::section::<PipelinesProbe>("pipelines")
                .map(|p| p.max_concurrent_runs)
                .unwrap_or(4)
        };
        Some(crate::pipeline::PipelineRuntime {
            engine: self.pipeline_engine.clone(),
            sink,
            hooks: self.hook_dispatcher.clone(),
            plugin_host: self.plugin_host.clone(),
            be_lua_op: self.build_be_lua_op_dispatch(),
            plugin_logs: self.plugin_logs.clone(),
            max_concurrent_runs,
        })
    }

    /// Build the worker-thread fallback closure that dispatches a `lua_op` step
    /// into the corvus-be plugin VM (where per-product plugins now register
    /// their ops) via the `invoke_pipeline_op` RPC. Returns `None` until the IPC
    /// router is wired (early boot). The closure captures the `Arc<Router>` —
    /// never the `AppHandle` — so it is `Send + Sync` and safe to run on the
    /// orchestrator worker thread; it mirrors `dispatch_rpc`'s call/parse body
    /// (serialize params → `router.call` → parse reply) without `&AppState`.
    fn build_be_lua_op_dispatch(&self) -> Option<crate::pipeline::BeLuaOpDispatch> {
        let router = self.router()?;
        let closure = move |plugin_name: &str,
                            op: &str,
                            params: serde_json::Value,
                            cwd: &str|
              -> std::result::Result<arbor_plugin_core::prelude::PipelineOpResult, String> {
            // The BE method wants the step params as a JSON *string*.
            let params_json = serde_json::to_string(&params)
                .map_err(|e| format!("lua_op params serialize: {e}"))?;
            let rpc_params = serde_json::json!({
                "plugin_name": plugin_name,
                "op": op,
                "params_json": params_json,
                "cwd": cwd,
            });
            let bytes = serde_json::to_vec(&rpc_params)
                .map_err(|e| format!("lua_op rpc encode: {e}"))?;
            let out = router
                .call("corvus", "invoke_pipeline_op", bytes)
                .map_err(|e| format!("lua_op rpc dispatch: {e:?}"))?;
            // Reply is `{ exit_code: i32, stdout: String, stderr: String }`.
            let v: serde_json::Value = if out.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&out)
                    .map_err(|e| format!("lua_op rpc decode: {e}"))?
            };
            Ok(arbor_plugin_core::prelude::PipelineOpResult {
                exit_code: v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                stdout: v.get("stdout").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                stderr: v.get("stderr").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            })
        };
        Some(std::sync::Arc::new(closure))
    }

    /// Repo registry guard — **reload-on-access**. corvus-be owns `repos.json`
    /// (ADR-1) and writes it from the other process; the shell still reads/writes
    /// it for the consumers that stay shell-side (deep-link router, missing-repo
    /// flow, `close_repo` orphan-GC, the `arbor.workspace` ns_shell namespace), so
    /// every guard reloads the file first — an in-memory cache would let the two
    /// processes drift and clobber each other. Writers mutate the guard and call
    /// `registry::save`; the next access re-reads it. Low-frequency, user-driven.
    pub fn lock_repo_registry(&self) -> Result<MutexGuard<'_, RepoRegistry>> {
        let mut g = self.repo_registry.lock().map_err(|e| {
            tracing::error!("repo_registry mutex poisoned: {e}");
            AppError::MutexPoisoned("repo_registry".into())
        })?;
        *g = crate::workspace::registry::load();
        Ok(g)
    }

    /// Workspace store guard — **reload-on-access** (see [`lock_repo_registry`]).
    pub fn lock_workspaces(&self) -> Result<MutexGuard<'_, WorkspaceStore>> {
        let mut g = self.workspaces.lock().map_err(|e| {
            tracing::error!("workspaces mutex poisoned: {e}");
            AppError::MutexPoisoned("workspaces".into())
        })?;
        *g = crate::workspace::store::load();
        Ok(g)
    }

    pub fn lock_git_providers(&self) -> Result<MutexGuard<'_, GitProviderRegistry>> {
        self.git_providers.lock().map_err(|e| {
            tracing::error!("git_providers mutex poisoned: {e}");
            AppError::MutexPoisoned("git_providers".into())
        })
    }

    pub fn new() -> Self {
        // Seed the active profile from the on-disk pointer before any
        // profile-scoped path resolves — the split config files live under
        // `arbor/profiles/<active>/` (docs/profiles-and-product-config.md).
        arbor_core::prelude::init_active_profile();
        // Relocate the pre-profiles flat satellite files (workspaces, repos,
        // session, …) into the active profile's corvus bucket before anything
        // reads them. One-shot + idempotent.
        crate::config::profile_migration::migrate_flat_satellites_to_active_profile();
        // Lift the now-shell-owned keys (git/terminals/activity_bar/ide/recent_repos)
        // out of corvus-be's `corvus/config.toml` into `profile.toml`, since those
        // sections left AppConfig and the shell no longer reads the corvus file.
        crate::config::profile_migration::migrate_generic_keys_out_of_corvus_config();
        let config = match crate::config::app_config::load() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to load app config, using defaults: {e}");
                AppConfig::default()
            }
        };
        // Resolve the git executable up-front so the very first git2/CLI call
        // sees the user's chosen binary, not a stale "git" placeholder.  When
        // nothing is found the GitSetupModal on the frontend prompts the user.
        {
            let configured = config.git.executable_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from);
            let snap = crate::git_cli::detect(configured.as_deref());
            match (&snap.path, snap.source) {
                (Some(p), Some(src)) => tracing::info!("git executable: {} ({src})", p.display()),
                _ => tracing::warn!("no git executable found — frontend will prompt"),
            }
        }
        let repo_registry = crate::workspace::registry::load();
        let workspaces    = crate::workspace::store::load();
        // Seed the GitProvider registry with the always-on hosts.  Self-hosted
        // GitLab instances are registered lazily on first use via
        // `git_provider::helpers::provider_for_tab`.
        let mut providers = GitProviderRegistry::new();
        providers.register(Arc::new(GithubProvider::new(
            Arc::new(crate::auth::vault::VaultSessionProvider::github()),
            "github.com",
        )));
        providers.register(Arc::new(GitlabProvider::new(
            Arc::new(crate::auth::vault::VaultSessionProvider::gitlab()),
        )));

        // Hook broker — built here (rather than in `setup()`) so the field can
        // stay an immutable `Arc<HookDispatcher>`: the static catalog and the
        // `LuaHookListener` (bound to the just-created `plugin_host`) are the
        // only inputs, and neither needs the Tauri `AppHandle`.
        let plugin_host = Arc::new(Mutex::new(PluginHost::new()));
        let hook_dispatcher = Arc::new(corvus_plugin::prelude::build_hook_dispatcher(&plugin_host));

        Self {
            plugin_host,
            hook_dispatcher,
            config:         Mutex::new(config),
            terminals:      Mutex::new(TerminalManager::new()),
            jobs:           Arc::new(Mutex::new(JobRegistry::default())),
            plugin_logs:    Arc::new(Mutex::new(PluginLogBuffer::default())),
            // Seed the registry with runs persisted on disk (terminal/resumable
            // ones — Running/Pending get coerced to Failed by `load_persisted_runs`).
            // The internal counter is advanced past the highest recovered id.
            pipeline_engine: Arc::new(crate::pipeline::PipelineEngine::new(
                crate::pipeline::registry_from_disk(),
            )),
            // Default to focused so schedulers fire normally until the
            // frontend sends the first focus update.
            app_focused:    Arc::new(AtomicBool::new(true)),
            active_tab_id:  Arc::new(Mutex::new(None)),
            active_repo_path: Arc::new(Mutex::new(None)),
            toolchain_registry: Arc::new(Mutex::new(ToolchainRegistry::new())),
            repo_registry:      Mutex::new(repo_registry),
            workspaces:         Mutex::new(workspaces),
            git_providers:          Mutex::new(providers),
            branding:               BrandingState::default(),
            deep_link_buffer:       Arc::new(DeepLinkBuffer::default()),
            studio_registry:        {
                let mut reg = StudioRegistry::new();
                reg.register(crate::ron_studio::backend_impl::backend());
                reg.register(crate::json_studio::backend_impl::backend());
                reg.register(crate::toml_studio::backend_impl::backend());
                reg.register(crate::yaml_studio::backend_impl::backend());
                reg.register(crate::properties_studio::backend_impl::backend());
                Arc::new(reg)
            },
            cloud_cancellations:    Arc::new(Mutex::new(HashMap::new())),
            streams:                Arc::new(crate::ipc::stream_registry::StreamRegistry::default()),
            cloud_pending_ops:      Arc::new(Mutex::new(HashMap::new())),
            cloud_host:             Arc::new(OnceLock::new()),
            brp:                    Mutex::new(BrpRegistry::default()),
            marketplace:            Mutex::new(crate::marketplace::build_registry()),
            boot_done:              Arc::new(AtomicBool::new(false)),
            boot_progress:          Arc::new(Mutex::new(None)),
            frontend_ready:         Arc::new((Mutex::new(false), Condvar::new())),
            scheduler:              Arc::new(OnceLock::new()),
            router:                 Arc::new(OnceLock::new()),
            corvus:                 Arc::new(OnceLock::new()),
        }
    }

    /// Re-resolve every per-profile cache against the now-active profile, for a
    /// **live** profile switch (`commands::profile_commands::switch_profile`).
    /// The active-profile cell must already point at the target, so the same
    /// loaders `new()` uses now read the new profile's files. Reloads the
    /// persistent state and drops session/per-tab state tied to the old
    /// profile's repos (the frontend reloads its stores, re-opening tabs from
    /// the new profile's `session.json`). The plugin host is reloaded separately
    /// by the caller via `reload_runtime`.
    pub fn reload_for_active_profile(&self) {
        // The newly-active profile may not have booted since the upgrade, so its
        // shell-owned keys could still sit in corvus-be's `corvus/config.toml`.
        // Lift them into `profile.toml` before the reload below reads it.
        crate::config::profile_migration::migrate_generic_keys_out_of_corvus_config();
        match crate::config::app_config::load() {
            Ok(c) => { if let Ok(mut g) = self.config.lock() { *g = c; } }
            Err(e) => tracing::warn!("profile switch: config reload failed: {e}"),
        }
        if let Ok(mut g) = self.repo_registry.lock() { *g = crate::workspace::registry::load(); }
        if let Ok(mut g) = self.workspaces.lock()    { *g = crate::workspace::store::load(); }
        if let Ok(mut g) = self.marketplace.lock()   { *g = crate::marketplace::build_registry(); }
        // Drop state bound to the old profile's open repos — the frontend
        // re-opens tabs after it reloads, which re-populates these. (The open-tab
        // → repo registry itself lives in corvus-be now, reset on its side via the
        // re-pushed profile paths below.)
        if let Ok(mut g) = self.terminals.lock()   { *g = TerminalManager::new(); }
        if let Ok(mut g) = self.brp.lock()         { *g = BrpRegistry::default(); }
        if let Ok(mut g) = self.active_tab_id.lock()   { *g = None; }
        if let Ok(mut g) = self.active_repo_path.lock() { *g = None; }
        // Re-push the now-active profile's resolved paths (corvus_config / git /
        // worktree-links / repo-registry / workspaces / workspace-state) to a live
        // corvus-be. It is a separate process that reads these files fresh on each
        // access but can't resolve the active profile itself — without this re-push
        // it stays pinned to the profile that was active when it spawned, so a live
        // profile switch would reload the shell (theme, config) yet still serve the
        // OLD profile's workspaces / repos. Best-effort: a no-op when corvus-be
        // isn't running (it gets the current paths from `ensure_corvus_be`'s own
        // `sync_config` when it later spawns). Must run after the registry/repo
        // reloads above so the pushed `repo_registry` reflects the new profile.
        crate::ipc::sync_config(self);
        // Repoint a live corvus-be at the new profile and reload its plugin host,
        // so the target profile's plugin set loads and re-emits its contributions
        // to the Corvus window (the FE reloads on `arbor://profile-switched`). The
        // launcher's own host is reloaded separately by `switch_profile`.
        crate::ipc::reload_corvus_plugins(self);
    }

    /// Shared trigger engine, once `setup()` has built it. Returns `None`
    /// during the brief window between `AppState::new()` and the scheduler
    /// being wired in — callers in that window should log + skip rather
    /// than panic.
    pub fn scheduler(&self) -> Option<Arc<Scheduler>> {
        self.scheduler.get().cloned()
    }

    /// The Model-D IPC router, once `setup()` has built it. Returns `None`
    /// during the brief window between `AppState::new()` and the router being
    /// wired in — commands only route after `setup()` returns, so in practice
    /// every command sees `Some`.
    pub fn router(&self) -> Option<Arc<Router>> {
        self.router.get().cloned()
    }

    /// The cloud host, once `cloud::install()` has built it. Returns `None`
    /// during the brief window between `AppState::new()` and `install()`
    /// completing — in practice every cloud command sees `Some`.
    pub fn cloud_host(&self) -> Option<Arc<dyn CloudHost>> {
        self.cloud_host.get().cloned()
    }

    /// Seed the headless Corvus backend state (the in-process `corvus-be`) and
    /// the Model-D IPC router into this state. Called once from `setup()` — both
    /// need the `AppHandle` (for the event sink / loopback dispatch) that
    /// `AppState::new()` predates. The event sink is backed by `AppHandle::emit`;
    /// the router fronts an in-process `LoopbackBroker`.
    pub fn wire_backend(&self, app: &tauri::AppHandle) {
        let sink: Arc<dyn EventSink> =
            Arc::new(crate::ipc::event_sink::TauriEventSink::new(app.clone()));
        let _ = self.corvus.set(
            CorvusState::new(sink).with_hooks(self.hook_dispatcher.clone()),
        );
        let router = crate::ipc::build_router(app);
        let _ = self.router.set(Arc::new(router));
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
