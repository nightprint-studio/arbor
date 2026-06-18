use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::sync::atomic::AtomicBool;
use std::collections::{HashMap, HashSet};
use tauri::Manager;

use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
#[cfg(any(not(debug_assertions), feature = "deep-link-dev"))]
use tauri_plugin_deep_link::DeepLinkExt;

mod app_ctx;
mod error;
mod explorer_window;
mod nemus;
mod nemus_window;
mod process_ext;
mod platform;
mod efficiency;
mod taskbar_icon_refresh;
mod git;
mod git_cli;
mod commands;
mod auth;
mod plugin;
mod config;
mod terminal;
mod jobs;
mod plugin_host_commands;
mod plugin_logs;
mod pipeline;
mod integrations;
mod workspace;
mod linked_worktrees;
mod git_provider;
mod provider_connect;
mod branding;
mod deep_link;
mod json_studio;
mod ron_studio;
mod toml_studio;
mod yaml_studio;
mod properties_studio;
mod studio;
mod cloud;
mod marketplace;
mod ipc;

use crate::error::{AppError, Result};
use crate::git::repo::RepoManager;
use crate::git::ticket_links::TicketLinkCache;
use arbor_plugin_core::prelude::{PluginHost, ToolchainRegistry};
use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};
use crate::config::app_config::AppConfig;
use crate::terminal::TerminalManager;
use crate::jobs::JobRegistry;
use crate::plugin_logs::PluginLogBuffer;
use crate::pipeline::PipelineRegistry;
use crate::workspace::{RepoRegistry, WorkspaceStore};
use crate::linked_worktrees::WorktreeLinkRegistry;
use crate::git_provider::{GitProviderRegistry, GithubProvider, GitlabProvider};
use crate::branding::BrandingState;
use crate::deep_link::DeepLinkBuffer;
use crate::studio::format::StudioRegistry;
use crate::cloud::{CloudCancellations, CloudPendingOps};
use arbor_cloud::host::CloudHost;
// `crate::cloud` is now a thin shim around the `arbor-cloud` workspace
// crate — see `cloud/mod.rs` for the layout / Phase A vs Phase B split.
use corvus_brp::prelude::BrpRegistry;
use arbor_plugin_marketplace::prelude::MarketplaceRegistry;
use arbor_shell_common::prelude::Router;
use std::sync::OnceLock;
use arbor_scheduler::prelude::Scheduler;

// ---------------------------------------------------------------------------
// Application state — shared across all Tauri commands
// ---------------------------------------------------------------------------

/// Build the [`HookDispatcher`]: register every static `HOOK_CATALOG` entry
/// (so introspection knows each hook's kind / ctx schema) and wire the single
/// mlua [`LuaHookListener`](arbor_plugin_core::prelude::LuaHookListener) bound
/// to `plugin_host`. `on_pre_commit` is the only vetoable hook today; the rest
/// are fire-and-forget.
fn build_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
    let mut dispatcher = HookDispatcher::new();
    for h in arbor_plugin_types::prelude::HOOK_CATALOG {
        dispatcher.register_hook(HookDef {
            name:        h.name,
            category:    h.category,
            description: h.description,
            kind:        if h.name == "on_pre_commit" {
                HookKind::Vetoable
            } else {
                HookKind::FireAndForget
            },
            ctx: h.ctx,
        });
    }
    dispatcher.register_listener(Arc::new(
        arbor_plugin_core::prelude::LuaHookListener::new(Arc::downgrade(plugin_host)),
    ));
    dispatcher
}

pub struct AppState {
    pub repos:          Mutex<RepoManager>,
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
    /// Per-tab ticket-link cache (auto-parsed + manual links).
    pub ticket_caches:  Mutex<std::collections::HashMap<String, TicketLinkCache>>,
    /// True when the app window has focus; used by focus-gated schedulers.
    pub app_focused:    Arc<AtomicBool>,
    /// The currently active tab ID as reported by the frontend.
    pub active_tab_id:  Arc<Mutex<Option<String>>>,
    /// Per-tab stats cache: tab_id — (head_sha, computed stats).
    /// Arc so background threads can hold a reference after the command returns.
    pub stats_cache: Arc<Mutex<HashMap<String, (String, crate::git::stats::RepoStats)>>>,
    /// Set of tab IDs currently being computed to prevent duplicate runs.
    pub stats_computing: Arc<Mutex<HashSet<String>>>,
    /// Installed toolchain registry (toolchains/<kind>.json).
    pub toolchain_registry: Arc<Mutex<ToolchainRegistry>>,
    /// Central registry of every repo Arbor has ever been shown.
    /// Referenced by workspaces by UUID — path edits flow from here.
    pub repo_registry: Mutex<RepoRegistry>,
    /// List of user-defined workspaces (plus the implicit Scratch one) and
    /// currently-active workspace id.  Tab snapshots live in separate files.
    pub workspaces:    Mutex<WorkspaceStore>,
    /// Report produced by the one-shot startup migration from legacy
    /// session.json.  `take()`-able: the welcome screen pulls it once on
    /// first launch after upgrade, leaving `None` for subsequent launches.
    pub migration_report: Mutex<Option<crate::workspace::migration::MigrationReport>>,
    /// Linked Worktrees — cross-project sync.  Persisted to linked_worktrees.toml.
    pub linked_worktrees: Mutex<WorktreeLinkRegistry>,
    /// Set of link ids currently being synced.  Read by the public checkout
    /// command to suppress recursive triggering of link sync from a
    /// propagated checkout (the orchestrator calls git ops directly, not the
    /// public command, so this guard is mostly defensive).
    pub link_sync_in_progress: Mutex<HashSet<String>>,
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
    pub cloud_host: Arc<std::sync::OnceLock<Arc<dyn CloudHost>>>,
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

    pub fn lock_repos(&self) -> Result<MutexGuard<'_, RepoManager>> {
        self.repos.lock().map_err(|e| {
            tracing::error!("repos mutex poisoned: {e}");
            AppError::MutexPoisoned("repos".into())
        })
    }

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
        let max_concurrent_runs = self.config.lock().ok()
            .map(|c| c.pipelines.max_concurrent_runs)
            .unwrap_or(4);
        Some(crate::pipeline::PipelineRuntime {
            engine: self.pipeline_engine.clone(),
            sink,
            hooks: self.hook_dispatcher.clone(),
            plugin_host: self.plugin_host.clone(),
            plugin_logs: self.plugin_logs.clone(),
            max_concurrent_runs,
        })
    }

    pub fn lock_ticket_caches(&self) -> Result<MutexGuard<'_, std::collections::HashMap<String, TicketLinkCache>>> {
        self.ticket_caches.lock().map_err(|e| {
            tracing::error!("ticket_caches mutex poisoned: {e}");
            AppError::MutexPoisoned("ticket_caches".into())
        })
    }

    pub fn lock_repo_registry(&self) -> Result<MutexGuard<'_, RepoRegistry>> {
        self.repo_registry.lock().map_err(|e| {
            tracing::error!("repo_registry mutex poisoned: {e}");
            AppError::MutexPoisoned("repo_registry".into())
        })
    }

    pub fn lock_workspaces(&self) -> Result<MutexGuard<'_, WorkspaceStore>> {
        self.workspaces.lock().map_err(|e| {
            tracing::error!("workspaces mutex poisoned: {e}");
            AppError::MutexPoisoned("workspaces".into())
        })
    }

    pub fn lock_linked_worktrees(&self) -> Result<MutexGuard<'_, WorktreeLinkRegistry>> {
        self.linked_worktrees.lock().map_err(|e| {
            tracing::error!("linked_worktrees mutex poisoned: {e}");
            AppError::MutexPoisoned("linked_worktrees".into())
        })
    }

    pub fn lock_git_providers(&self) -> Result<MutexGuard<'_, GitProviderRegistry>> {
        self.git_providers.lock().map_err(|e| {
            tracing::error!("git_providers mutex poisoned: {e}");
            AppError::MutexPoisoned("git_providers".into())
        })
    }

    fn new() -> Self {
        let config = match config::app_config::load() {
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
            let snap = git_cli::detect(configured.as_deref());
            match (&snap.path, snap.source) {
                (Some(p), Some(src)) => tracing::info!("git executable: {} ({src})", p.display()),
                _ => tracing::warn!("no git executable found — frontend will prompt"),
            }
        }
        // Run the one-shot workspace migration before loading the current
        // registry/store so we don't race with a partial write from a crash.
        let migration_report = crate::workspace::migration::run_if_needed();
        let repo_registry = crate::workspace::registry::load();
        let workspaces    = crate::workspace::store::load();
        // Only keep the report around if it actually represents work done.
        // `already_migrated` means both files existed — nothing to surface.
        let stored_report = if migration_report.already_migrated { None } else { Some(migration_report) };
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
        let hook_dispatcher = Arc::new(build_hook_dispatcher(&plugin_host));

        Self {
            repos:          Mutex::new(RepoManager::new()),
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
            ticket_caches:  Mutex::new(std::collections::HashMap::new()),
            // Default to focused so schedulers fire normally until the
            // frontend sends the first focus update.
            app_focused:    Arc::new(AtomicBool::new(true)),
            active_tab_id:  Arc::new(Mutex::new(None)),
            stats_cache:    Arc::new(Mutex::new(HashMap::new())),
            stats_computing: Arc::new(Mutex::new(HashSet::new())),
            toolchain_registry: Arc::new(Mutex::new(ToolchainRegistry::new())),
            repo_registry:      Mutex::new(repo_registry),
            workspaces:         Mutex::new(workspaces),
            migration_report:   Mutex::new(stored_report),
            linked_worktrees:       Mutex::new(crate::linked_worktrees::load()),
            link_sync_in_progress:  Mutex::new(HashSet::new()),
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
            cloud_host:             Arc::new(std::sync::OnceLock::new()),
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
}

// ---------------------------------------------------------------------------
// Tauri entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Single-instance + deep-link: the main Arbor UI must never run as a second
    // process / second window — a duplicate launch (incl. every `arbor://—` URL
    // invocation) short-circuits and just focuses the running instance's `main`
    // window. Only the dedicated File Explorer window (`explorer-*`) is allowed
    // to be multi-window, and that's an in-process concern, not a second
    // instance.
    //
    // This is **always on in release** (the actual app the user runs), but
    // intentionally **OFF in plain `cargo tauri dev`**: the single-instance lock
    // fights the dev runner's rebuild/relaunch cycle — on relaunch the new
    // process detects the still-running prior dev process as the primary, calls
    // the callback and exits immediately, leaving the terminal detached and a
    // stale (blank) webview behind. Opt in for dev with the `deep-link-dev`
    // Cargo feature when you specifically need to test single-instance / deep
    // links.
    //
    // Single-instance MUST be the FIRST plugin: a duplicate launch needs to
    // short-circuit before any other setup runs. The `deep-link` feature on
    // `tauri-plugin-single-instance` makes the forwarded argv flow straight into
    // the deep-link plugin's `on_open_url` callback registered in `setup()`.
    #[cfg(any(not(debug_assertions), feature = "deep-link-dev"))]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }))
            .plugin(tauri_plugin_deep_link::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // OS-global shortcut (Ctrl+Shift+E) — dedicated File Explorer window.
        // The handler only reacts on key-down for our one registered combo;
        // the combo itself is registered in `setup()` below.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state() == ShortcutState::Pressed {
                        if let Some(sc) = crate::explorer_window::current_explorer_shortcut() {
                            if shortcut == &sc {
                                crate::explorer_window::open_or_focus(app);
                            }
                        }
                    }
                })
                .build(),
        )
        .manage(AppState::new())
        .manage(explorer_window::PendingReveals::default())
        .manage(explorer_window::ExplorerClipboard::default())
        .manage(explorer_window::DragOverlayText::default())
        .manage(nemus::NemusState::default())
        .setup(|app| {
            // One-time storage split: move nemus's data out of the old
            // `<arbor-data>/nemus` tree into its own `<nemus-data>` root and seed
            // nemus's standalone config from Arbor's legacy `[nemus]` section.
            // Cheap no-op once migrated; runs before the nemus window can open.
            crate::nemus::migrate_storage();

            // Build the Model-D IPC router (M3 Asse B) and publish it into
            // AppState. Today it fronts an in-process `LoopbackBroker` capturing
            // this `AppHandle`; the same router will front a pipe/socket client
            // once the product backends split out. Must run before any command
            // routes — safe here because commands only fire once `Builder::run()`
            // enters its event loop, after `setup()` returns.
            {
                let state = app.state::<AppState>();
                // Seed the Corvus backend state (the in-process `corvus-be`): its
                // first piece is the event egress, backed here by `AppHandle::emit`.
                // Model-D handlers reached through the generic `rpc` command hold
                // only `&AppState` and push events via `AppState::emit`, which
                // routes through this `CorvusState`.
                let sink: std::sync::Arc<dyn arbor_ipc::prelude::EventSink> =
                    std::sync::Arc::new(crate::ipc::event_sink::TauriEventSink::new(app.handle().clone()));
                let _ = state.corvus.set(corvus_core::prelude::CorvusState::new(sink));
                let router = crate::ipc::build_router(app.handle());
                let _ = state.router.set(std::sync::Arc::new(router));
            }

            // Wire the `arbor-cloud` crate against AppState: registers the
            // Google OAuth refresher and publishes the `Arc<dyn CloudHost>`
            // into Tauri state so command + plugin-namespace layers can pull
            // it back out. Must run after the event sink is wired above (the
            // host stores the sink for `emit_event`). Safe before commands
            // fire — commands only route once `Builder::run()` enters its
            // event loop, which happens after `setup()` returns.
            crate::cloud::install(&app.handle());

            // Register the configured OS-global File-Explorer shortcut (opt-in;
            // no-op when disabled or unset). The press handler is wired on the
            // plugin builder above; here we just claim the configured combo.
            #[cfg(desktop)]
            crate::explorer_window::register_configured(app.handle());

            // Register the `arbor://` URI scheme at runtime.  This is what
            // makes deep links work in `--no-bundle` builds where there is no
            // installer to write the registry entry — every dev launch points
            // the scheme at the binary that just started.  No-op on platforms
            // where the bundler/OS already owns the registration.
            //
            // URLs received here are routed through `DeepLinkBuffer`, which
            // either emits to the frontend immediately (warm path — listener
            // is mounted) or buffers until `deep_link_ready` flushes (cold
            // start — webview hasn't booted yet).
            #[cfg(all(desktop, any(not(debug_assertions), feature = "deep-link-dev")))]
            {
                if let Err(e) = app.deep_link().register("arbor") {
                    tracing::warn!("failed to register arbor:// scheme: {e}");
                }
                let handle_dl = app.handle().clone();
                let buffer    = app.state::<AppState>().deep_link_buffer.clone();

                // Runtime opens (warm path + URLs forwarded by the
                // single-instance plugin's `deep-link` feature).
                let buffer_for_runtime = buffer.clone();
                let handle_for_runtime = handle_dl.clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        tracing::info!("deep-link received: {url}");
                        buffer_for_runtime.push_or_emit(&handle_for_runtime, url.to_string());
                    }
                    if let Some(w) = handle_for_runtime.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                });

                // Cold-start URLs — when the OS launched Arbor by clicking a
                // link, the URL is sitting in argv but `on_open_url` may not
                // re-fire for it depending on the platform.  Drain
                // `get_current()` defensively into the same buffer.
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    for url in urls {
                        tracing::info!("deep-link cold-start: {url}");
                        buffer.push_or_emit(&handle_dl, url.to_string());
                    }
                }
            }

            // Shared trigger engine (`arbor-scheduler`). Built once on the
            // tauri-managed Tokio runtime + wired into `AppState` and
            // `PluginHost` BEFORE either the marketplace auto-refresh or
            // the plugin boot thread tries to register against it.
            {
                let state = app.state::<AppState>();

                // Tauri's `async_runtime::spawn` is usable from sync
                // `setup()` and runs the future on its internal Tokio
                // runtime — capture `Handle::current()` from inside that
                // future to get the runtime handle the scheduler needs.
                let (tx, rx) = std::sync::mpsc::sync_channel::<tokio::runtime::Handle>(1);
                tauri::async_runtime::spawn(async move {
                    let _ = tx.send(tokio::runtime::Handle::current());
                });
                let rt_handle = rx.recv()
                    .expect("could not capture tokio runtime handle for arbor-scheduler");

                let ctx: Arc<dyn arbor_core::prelude::AppCtx> = Arc::new(
                    crate::app_ctx::TauriAppCtx::new(
                        app.handle().clone(),
                        state.app_focused.clone(),
                    )
                );

                // Hand the host context + the Lua API installer to
                // PluginHost. `set_app_ctx` also routes the AppCtx into the
                // ContributionRegistry so the coalesced
                // `arbor://contributions-changed` /
                // `arbor://containers-changed` emits stay routed to the
                // frontend (PR #4 — `arbor-plugin-core` migration).
                {
                    let mut host = state
                        .plugin_host
                        .lock()
                        .expect("plugin_host poisoned at AppCtx install");
                    host.set_app_ctx(ctx.clone());
                    host.set_api_installer(
                        crate::plugin::api_installer::tauri_api_installer(),
                    );
                    // Marketplace install dir is scanned alongside the host's
                    // dev `plugin_dir()` during reload. Passed as an extra
                    // root so `arbor-plugin-core` itself stays free of any
                    // marketplace coupling.
                    host.set_extra_plugin_roots(vec![
                        arbor_plugin_marketplace::prelude::plugins_dir(),
                    ]);
                }

                let scheduler = Arc::new(Scheduler::new(ctx, rt_handle));
                let _ = state.scheduler.set(scheduler.clone());

                // Hand the scheduler + a weak self-pointer to PluginHost so
                // Lua-fired actions can call back into `hook_router::fire_on`.
                let host_arc = state.plugin_host.clone();
                {
                    let mut host = host_arc.lock()
                        .expect("plugin_host poisoned during scheduler install");
                    host.install_scheduler(scheduler, Arc::downgrade(&host_arc));
                }
            }

            // Marketplace auto-refresh — one entry in the shared engine.
            // Settings reads ride the `gate` closure so toggling
            // `refresh_hours` / `poll_minutes` reconfigures on the fly.
            crate::marketplace::scheduler::install(app.handle().clone());

            // Plugin loading moved to a background thread so the webview can
            // mount + render its boot-splash overlay BEFORE the (potentially
            // slow) discover — topo-sort — `load_plugin` pass blocks the
            // UI thread. The thread emits `arbor://boot-progress` events per
            // plugin and a final `arbor://boot-done` event for the splash to
            // dismiss itself.
            //
            // Even though we now run async w.r.t. the main thread, callers of
            // commands like `list_plugins` will still see consistent state:
            // the mutex they take here serialises every plugin-touching IPC
            // against the boot loader, so a frontend command issued before
            // boot completes simply waits on the mutex.
            let handle_for_boot = app.handle().clone();
            // Synchronous handshake: setup() returns ONLY after the boot
            // thread has acquired the plugin_host mutex. Without this gate,
            // there's a window between `thread::spawn` returning and the OS
            // actually scheduling the boot thread — during which the WebView
            // can mount and AppShell.onMount can fire IPCs (list_plugin_info,
            // list_plugin_contributions) that win the lock first, find an
            // empty host, and seed frontend stores with empty state.
            let (lock_acquired_tx, lock_acquired_rx) =
                std::sync::mpsc::sync_channel::<()>(0);
            std::thread::Builder::new()
                .name("arbor-plugin-boot".to_string())
                .spawn(move || {
                    use tauri::Emitter;
                    let state = handle_for_boot.state::<AppState>();
                    let mut host = state
                        .plugin_host
                        .lock()
                        .expect("plugin_host mutex poisoned during boot");
                    // Signal setup() that the lock is now held by us. From
                    // this point on, every frontend IPC that needs
                    // `plugin_host` queues behind us. `send` blocks until
                    // setup() calls `recv`, so this is a true rendezvous.
                    let _ = lock_acquired_tx.send(());

                    // Wait for the frontend handshake before emitting any
                    // boot events. `BootSplash.onMount` registers the
                    // `arbor://boot-progress` + `arbor://boot-done` listeners,
                    // then calls the `frontend_ready` IPC which flips this
                    // flag. Without the handshake, fast boots can emit and
                    // dismiss before listeners exist; with it, events always
                    // land. The 5s timeout is a safety net so a wedged or
                    // missing frontend can't strand the boot thread forever.
                    {
                        let (lock, cvar) = &*state.frontend_ready;
                        let mut ready = lock.lock()
                            .expect("frontend_ready mutex poisoned during boot");
                        let timeout = std::time::Duration::from_secs(5);
                        let deadline = std::time::Instant::now() + timeout;
                        while !*ready {
                            let remaining = deadline.saturating_duration_since(
                                std::time::Instant::now(),
                            );
                            if remaining.is_zero() {
                                tracing::warn!(
                                    "frontend_ready handshake timed out after 5s — proceeding (BootSplash will recover via get_boot_state)"
                                );
                                break;
                            }
                            let (g, wait_res) = cvar.wait_timeout(ready, remaining)
                                .expect("frontend_ready condvar wait poisoned");
                            ready = g;
                            if wait_res.timed_out() && !*ready {
                                tracing::warn!(
                                    "frontend_ready handshake timed out after 5s — proceeding (BootSplash will recover via get_boot_state)"
                                );
                                break;
                            }
                        }
                    }

                    // PluginHost's app context / api installer / extra
                    // roots are wired up in the early setup block above —
                    // before this thread acquires the lock — so the boot
                    // thread goes straight to `reload()`.

                    let plugins_enabled = state
                        .config
                        .lock()
                        .map(|c| c.plugins_enabled)
                        .unwrap_or(false);

                    // Helper closure: emit the live event AND mirror the
                    // payload into shared state so the splash can recover
                    // when the WebView mounts after the event has fired.
                    let emit_progress = |payload: serde_json::Value| {
                        if let Ok(mut slot) = state.boot_progress.lock() {
                            *slot = Some(payload.clone());
                        }
                        let _ = handle_for_boot.emit("arbor://boot-progress", payload);
                    };
                    let mark_done = |payload: serde_json::Value| {
                        state.boot_done.store(true, std::sync::atomic::Ordering::Release);
                        if let Ok(mut slot) = state.boot_progress.lock() {
                            *slot = Some(payload.clone());
                        }
                        let _ = handle_for_boot.emit("arbor://boot-done", payload);
                    };

                    if !plugins_enabled {
                        tracing::info!("plugin system disabled by config — skipping load");
                        mark_done(serde_json::json!({
                            "skipped": true,
                            "reason":  "plugin system disabled in config",
                        }));
                        return;
                    }

                    if let Err(e) = host.reload() {
                        tracing::warn!("failed to load plugins during boot: {e}");
                        emit_progress(serde_json::json!({
                            "phase":   "reload-error",
                            "message": format!("Plugin discovery failed: {e}"),
                        }));
                    }

                    emit_progress(serde_json::json!({
                        "phase":   "starting-schedulers",
                        "message": "Starting plugin schedulers—",
                    }));
                    host.start_all_schedulers();

                    // Match the manual `reload_plugins` command: emit
                    // `arbor://plugins-reloaded` so every store/component that
                    // refreshes on that signal (contributionStore, pluginStore
                    // via PluginPanel, containerStore, depsExplorerStore,
                    // DocsPanel, PluginSidebarPanel, —) re-reads with the
                    // host fully populated. Without this, listeners attached
                    // during AppShell mount sit idle waiting for an event
                    // that only the manual Refresh button would fire.
                    let _ = handle_for_boot.emit("arbor://plugins-reloaded", ());

                    mark_done(serde_json::json!({
                        "skipped": false,
                    }));
                })
                .expect("failed to spawn arbor-plugin-boot thread");

            // Block setup() here until the boot thread has acquired the
            // plugin_host lock. The send() in the thread is the rendezvous
            // point: after this returns, every plugin-touching IPC issued
            // by the frontend is guaranteed to queue behind boot.
            lock_acquired_rx
                .recv()
                .expect("arbor-plugin-boot thread exited before signalling lock acquisition");

            // Efficiency-mode driver. The whole-system process scan that
            // applies EcoQoS runs on a dedicated worker thread that coalesces
            // focus/resize bursts and re-scans periodically while unfocused
            // (to catch renderers spawned in the background). Window events
            // below only signal the desired state — they never scan on the
            // UI/event thread, which previously froze the app on resume.
            crate::efficiency::init();

            // Re-apply the main window icon after sleep/resume — works
            // around a Windows + WebView2 quirk that drops the taskbar's
            // small HICON on wake. Active in debug and release alike since
            // the bug is OS-level.
            crate::taskbar_icon_refresh::install(app.handle());

            // System tray — only in release builds
            #[cfg(not(debug_assertions))]
            {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let show = MenuItem::with_id(app, "show", "Show Arbor", true, None::<&str>)?;
                let explorer = MenuItem::with_id(app, "explorer", "Open File Explorer", true, None::<&str>)?;
                let sep = PredefinedMenuItem::separator(app)?;
                let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &explorer, &sep, &quit])?;

                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .tooltip("Arbor")
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "explorer" => crate::explorer_window::open_or_focus(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // The nemus window closing for real tears down its audio
                    // session (drops the cpal stream on the audio thread, stops
                    // sound). Lazy ownership: nothing happens if it never played.
                    if window.label() == crate::nemus_window::NEMUS_WINDOW_LABEL {
                        crate::nemus::shutdown(window.app_handle());
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        // Close-to-tray applies ONLY to the main window. Auxiliary
                        // windows (the dedicated File Explorer, the drag-ghost
                        // overlay) close for real — otherwise a closed explorer is
                        // merely hidden and reopening re-summons the same stale
                        // window instead of a fresh one.
                        if window.label() == "main" {
                            api.prevent_close();
                            let _ = window.hide();
                        }
                    }
                    #[cfg(debug_assertions)]
                    let _ = api;
                }
                tauri::WindowEvent::Focused(focused) => {
                    let focused = *focused;
                    // Update the app-focused flag so focus-gated schedulers work correctly.
                    let state = window.app_handle().state::<AppState>();
                    state.app_focused.store(focused, std::sync::atomic::Ordering::Relaxed);
                    // Signal the desired OS power-throttle state (EcoQoS on Windows,
                    // nice/sched on Linux/macOS). Handled here in the native
                    // window-event callback rather than via a frontend IPC call so
                    // minimize / Alt-Tab / window-switch are all caught reliably via
                    // Win32 WM_SETFOCUS / WM_KILLFOCUS messages. The actual (expensive)
                    // process scan runs off-thread in the efficiency worker.
                    crate::efficiency::request(!focused);
                }
                tauri::WindowEvent::Resized(size) => {
                    // Windows reports minimize as a Resized event with width=0, height=0.
                    // Focused(false) alone doesn't always fire on minimize (depending on
                    // desktop/window-manager behavior), so we trigger efficiency mode
                    // from here too as a belt-and-braces catch.
                    if size.width == 0 && size.height == 0 {
                        let state = window.app_handle().state::<AppState>();
                        state.app_focused.store(false, std::sync::atomic::Ordering::Relaxed);
                        crate::efficiency::request(true);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Repo
            commands::repo_commands::init_repo,
            commands::repo_commands::clone_repo,
            // Graph (read ops migrated to corvus; streaming/job ones deferred)
            // Diff (read ops migrated to corvus; streaming ones deferred)
            // Stage
            // Branches
            commands::branch_commands::delete_branch,
            commands::branch_commands::rename_branch,
            commands::branch_commands::checkout_branch,
            commands::branch_commands::checkout_branch_safe,
            commands::branch_commands::checkout_remote_as_local,
            commands::branch_commands::checkout_remote_as_local_safe,
            // Remote — fetch/push/pull migrated to the generic router
            // (ipc::corvus::remote); open_in_browser stays (OS opener glue).
            // Generic Model-D IPC entry point — the FE forwards every product
            // command here as (program, method, params); the shell router
            // dispatches to the right backend. Migrated domains route through
            // here (handlers in crate::ipc::corvus::*): stash, reset/tags,
            // notes, reflog, bisect. The rest below are still inline
            // #[tauri::command]s, migrating domain by domain. See crate::ipc.
            commands::rpc_commands::rpc,
            // Auth + provider (credential store, OAuth start, descriptors)
            // migrated to corvus handlers (see crate::ipc::corvus::{auth, provider}).
            // Plugins
            commands::plugin_commands::set_plugins_enabled,
            commands::plugin_commands::reload_plugins,
            commands::plugin_commands::exec_hook,
            commands::plugin_commands::fire_plugin_action,
            commands::plugin_commands::fire_command,
            commands::plugin_commands::enable_plugin,
            commands::plugin_commands::disable_plugin,
            commands::plugin_commands::delete_plugin,
            commands::plugin_commands::start_plugin_scheduler,
            commands::plugin_commands::stop_plugin_scheduler,
            // Session persistence
            // Workspaces
            commands::workspace_commands::delete_workspace,
            commands::workspace_commands::set_active_workspace,
            commands::workspace_commands::remove_repo_from_workspace,
            commands::workspace_commands::delete_registry_repo,
            commands::workspace_commands::import_workspace_commit,
            commands::workspace_commands::import_workspace_group_commit,
            commands::workspace_commands::import_bundle_commit,
            commands::workspace_commands::workspace_fetch_all,
            commands::workspace_commands::workspace_pull_all,
            commands::workspace_commands::workspace_tag_all,
            // Per-repo config
            // Recent repos (app-level config)
            // Graph config
            // Graph column layout (separate TOML)
            // Cache config
            // Pipelines orchestrator config
            // Issues config
            // MR/PR Activity timeline defaults
            // Appearance preferences (window control style, font scale, —)
            commands::config_commands::set_explorer_config,
            // UI animations preferences (enabled, speed)
            // Commit preferences (host-wide template fallback, —)
            // First-run onboarding tour state
            // "What's New" modal state (last-seen app version)
            // Branches sidebar (global behaviour + per-repo grouping state)
            // Activity bar config
            // Diff config (algorithm, context, full-file, virt threshold)
            // Missing-projects config (tombstone + locate)
            // OAuth client-id overrides
            // Terminal
            commands::terminal_commands::terminal_create,
            // Jobs
            commands::job_commands::cancel_job,
            // Plugin logs (arbor.log.* ring buffer)
            // App focus / active-tab state (used by focus-gated schedulers)
            commands::plugin_commands::set_app_focus,
            commands::plugin_commands::set_active_tab,
            // Boot state — polled by BootSplash to recover from listener-timing race
            commands::plugin_commands::get_boot_state,
            // Boot handshake — BootSplash flips this once listeners are attached
            commands::plugin_commands::frontend_ready,
            // Toolchains
            // Cross-plugin contribution model — tree snapshots and custom
            // icons are read through the unified registry, no parallel IPC.
            // Container model (Phase 2 — ContributableModal)
            // Open in browser
            commands::remote_commands::open_in_browser,
            // Pipeline engine: all handlers (queries, cancel, run/request/
            // resume/discard) migrated to corvus handlers — the orchestrator
            // now takes an injected `PipelineRuntime` instead of an AppHandle.
            // Pipelines (CI/CD) + Security dashboard migrated to corvus handlers.
            // Filesystem browser
            commands::fs_commands::fs_set_wallpaper,
            commands::fs_commands::fs_open_default,
            commands::fs_commands::fs_reveal_in_dir,
            commands::fs_commands::fs_open_terminal,
            commands::fs_commands::fs_show_properties,
            commands::fs_commands::fs_icon,
            commands::fs_commands::fs_watch_start,
            commands::fs_commands::fs_watch_stop,
            // File Explorer git awareness — status overlays, inline actions,
            // and "Open in Arbor" delegation for the heavy git operations.
            commands::fs_git_commands::fs_git_status,
            commands::fs_git_commands::fs_git_stage,
            commands::fs_git_commands::fs_git_unstage,
            commands::fs_git_commands::fs_git_discard,
            commands::fs_git_commands::fs_git_ignore,
            commands::fs_git_commands::fs_git_checkout,
            commands::fs_git_commands::fs_open_in_arbor,
            // Avatar resolution via GitProvider (GitHub + GitLab)
            // Merge Requests / Pull Requests + Issues (Linear/Jira) migrated to
            // corvus handlers (conflict-resolution now streams via arbor-ipc Stream).
            // Inline image proxy + remote repository browser + portable-git
            // download all migrated to corvus handlers (ipc::corvus::{image,
            // repo_browser, git_cli}).
            // Deep-link router (arbor:// URLs)
            commands::deep_link_commands::deep_link_ready,
            commands::deep_link_commands::dispatch_deep_link,
            // Missing-repo tombstone + locate
            // Studio Multi-Format backbone migrated to studio handlers.
            // studio sidebar — project-wide .ron/.json/.toml index.
            // cloud-storage plugin migrated to platform handlers.
            // Bevy Remote Protocol (Phase 1.0 — read-only HTTP)
            commands::brp_commands::brp_connect,
            commands::brp_commands::brp_call,
            // Marketplace
            commands::marketplace_commands::marketplace_fetch_registry,
            commands::marketplace_commands::marketplace_refresh_registry,
            commands::marketplace_commands::marketplace_set_refresh_hours,
            commands::marketplace_commands::marketplace_set_poll_minutes,
            commands::marketplace_commands::marketplace_install_plugin,
            commands::marketplace_commands::marketplace_uninstall_plugin,
            commands::marketplace_commands::marketplace_set_plugin_enabled,
            commands::marketplace_commands::marketplace_install_theme,
            commands::marketplace_commands::marketplace_uninstall_theme,
            commands::marketplace_commands::marketplace_add_custom_source,
            // Dedicated File Explorer window
            explorer_window::open_explorer_window,
            explorer_window::reveal_in_explorer,
            explorer_window::take_explorer_reveal,
            // Cross-window clipboard + drag/drop (between explorer windows)
            explorer_window::explorer_clip_set,
            explorer_window::explorer_clip_get,
            explorer_window::explorer_clip_clear,
            explorer_window::get_drag_overlay_text,
            explorer_window::ensure_drag_overlay,
            explorer_window::drag_overlay_show,
            explorer_window::drag_overlay_move,
            explorer_window::drag_overlay_hide,
            explorer_window::explorer_drop_dispatch,
            // Dedicated nemus (music live-coding) window
            nemus_window::open_nemus_window,
            // nemus engine: eval / transport / render / sample packs / config
            nemus::nemus_eval,
            nemus::nemus_transport,
            nemus::nemus_render,
            nemus::nemus_render_stems,
            nemus::nemus_export_midi,
            nemus::nemus_analyze_levels,
            nemus::nemus_packs,
            nemus::nemus_pack_download,
            nemus::nemus_pack_reindex,
            nemus::nemus_pack_delete,
            nemus::get_nemus_config,
            nemus::set_nemus_config,
            nemus::nemus_audio_devices,
            nemus::nemus_set_output_device,
            // nemus Fase 4: arrangement query / sound bank / live mixer /
            // window state / project model (all additive)
            nemus::query::nemus_query,
            nemus::scenes::nemus_scenes,
            nemus::scenes::nemus_launch,
            nemus::sounds::nemus_sounds,
            nemus::nemus_set_track,
            nemus::nemus_audition_expr,
            nemus::nemus_eval_snippet,
            nemus::nemus_materialize,
            nemus::nemus_play_snippet,
            nemus::nemus_stop_snippet,
            nemus::state::get_nemus_state,
            nemus::state::set_nemus_state,
            nemus::state::get_nemus_project_tabs,
            nemus::state::set_nemus_project_tabs,
            nemus::state::get_nemus_project_mix,
            nemus::state::set_nemus_project_mix,
            nemus::state::get_nemus_aliases,
            nemus::state::set_nemus_aliases,
            nemus::state::get_nemus_scratch_tabs,
            nemus::state::set_nemus_scratch_tabs,
            nemus::project::nemus_open_project,
            nemus::project::nemus_create_project,
            nemus::project::nemus_set_project_name,
            nemus::reference::nemus_lang_reference,
            nemus::format::nemus_format,
            nemus::scales::nemus_scales,
            nemus::libraries::nemus_libraries,
            nemus::libraries::nemus_sync_libraries,
            // nemus import: WAV — MIDI (transcription) / MIDI — .nemus (deterministic)
            nemus::import::nemus_convert_wav_to_midi,
            nemus::import::nemus_import_audio_as_nemus,
            nemus::import::nemus_import_midi_as_nemus,
            // nemus ONNX transcription models (download on-demand)
            nemus::models::nemus_models,
            nemus::models::nemus_download_model,
            nemus::models::nemus_delete_model,
        ])
    .run(tauri::generate_context!())
        .expect("error while running arbor");
}
