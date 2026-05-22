use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::AtomicBool;
use std::collections::{HashMap, HashSet};
use tauri::Manager;
#[cfg(any(not(debug_assertions), feature = "deep-link-dev"))]
use tauri_plugin_deep_link::DeepLinkExt;

mod error;
mod process_ext;
mod platform;
mod git;
mod git_cli;
mod commands;
mod auth;
mod plugin;
mod config;
mod terminal;
mod jobs;
mod plugin_logs;
mod pipeline;
mod integrations;
mod workspace;
mod linked_worktrees;
mod git_provider;
mod branding;
mod deep_link;
mod json_studio;
mod ron_studio;
mod toml_studio;
mod yaml_studio;
mod properties_studio;
mod studio;
mod cloud;
mod brp;
mod marketplace;

use crate::error::{AppError, Result};
use crate::git::repo::RepoManager;
use crate::git::ticket_links::TicketLinkCache;
use crate::plugin::runtime::PluginHost;
use crate::config::app_config::AppConfig;
use crate::terminal::TerminalManager;
use crate::jobs::JobRegistry;
use crate::plugin_logs::PluginLogBuffer;
use crate::pipeline::PipelineRegistry;
use crate::plugin::toolchain::ToolchainRegistry;
use crate::workspace::{RepoRegistry, WorkspaceStore};
use crate::linked_worktrees::WorktreeLinkRegistry;
use crate::git_provider::{GitProviderRegistry, GithubProvider, GitlabProvider};
use crate::branding::BrandingState;
use crate::deep_link::DeepLinkBuffer;
use crate::studio::format::StudioRegistry;
use crate::cloud::{CloudCancellations, CloudPendingOps};
use crate::brp::BrpRegistry;
use crate::marketplace::MarketplaceRegistry;

// ---------------------------------------------------------------------------
// Application state — shared across all Tauri commands
// ---------------------------------------------------------------------------

pub struct AppState {
    pub repos:          Mutex<RepoManager>,
    pub plugin_host:    Mutex<PluginHost>,
    pub config:         Mutex<AppConfig>,
    pub terminals:      Mutex<TerminalManager>,
    pub jobs:           Mutex<JobRegistry>,
    /// Ring-buffer of recent `arbor.log.*` entries from every plugin —
    /// powers the Plugin Logs bottom panel.
    pub plugin_logs:    Mutex<PluginLogBuffer>,
    pub pipelines:      Mutex<PipelineRegistry>,
    /// Wakes orchestrator threads queued behind the global concurrency cap
    /// (`config.pipelines.max_concurrent_runs`).  Notified whenever a run
    /// transitions out of `Running` so the next queued run can take its
    /// slot. Always paired with `pipelines` — wait-protocol requires
    /// holding the registry lock around `wait_timeout`.
    pub pipeline_cv:    Arc<std::sync::Condvar>,
    /// Per-tab ticket-link cache (auto-parsed + manual links).
    pub ticket_caches:  Mutex<std::collections::HashMap<String, TicketLinkCache>>,
    /// True when the app window has focus; used by focus-gated schedulers.
    pub app_focused:    Arc<AtomicBool>,
    /// The currently active tab ID as reported by the frontend.
    pub active_tab_id:  Arc<Mutex<Option<String>>>,
    /// Per-tab stats cache: tab_id → (head_sha, computed stats).
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
    /// Unified registry of remote git host clients (GitHub / GitLab / …).
    /// Populated at boot — see `git_provider/`.
    pub git_providers: Mutex<GitProviderRegistry>,
    /// In-memory branding overrides applied by plugins (logo, etc.).
    pub branding: BrandingState,
    /// Cold-start buffer for `arbor://…` URLs received before the frontend
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
    pub cloud_cancellations: CloudCancellations,
    /// stream_id → JobRegistry job_id for `download_many` calls with
    /// `keep_open=true` (chunk-merge flow). `cloud_report_done` reads +
    /// removes the entry to finalize the job once the merge phase ends.
    pub cloud_pending_ops: CloudPendingOps,
    /// Bevy Remote Protocol — singleton live session against one Bevy game
    /// at a time. Read-only HTTP for Phase 1; SSE watch + editing in later
    /// phases. See `project_bevy_brp_client.md` memory.
    pub brp: Mutex<BrpRegistry>,
    /// Plugin & theme marketplace registry. Phase 1 is an in-memory seeded
    /// catalog (mock data); later phases swap the loader for a GitHub fetcher
    /// + user_registry.toml persistence.
    pub marketplace: Mutex<MarketplaceRegistry>,
    /// Mirrors the `arbor://boot-progress` / `arbor://boot-done` event stream
    /// in shared state so the splash can recover from the dev-mode race where
    /// the WebView mounts (and registers its listener) AFTER the boot thread
    /// has already finished and emitted `boot-done`. `BootSplash.svelte` polls
    /// `get_boot_state` on mount and dismisses immediately when `done == true`.
    pub boot_done:     Arc<AtomicBool>,
    pub boot_progress: Arc<Mutex<Option<serde_json::Value>>>,
}

impl AppState {
    // ── Mutex lock helpers ───────────────────────────────────────────────────
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
        self.pipelines.lock().map_err(|e| {
            tracing::error!("pipelines mutex poisoned: {e}");
            AppError::MutexPoisoned("pipelines".into())
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
        providers.register(Arc::new(GithubProvider::new()));
        providers.register(Arc::new(GitlabProvider::new()));

        Self {
            repos:          Mutex::new(RepoManager::new()),
            plugin_host:    Mutex::new(PluginHost::new()),
            config:         Mutex::new(config),
            terminals:      Mutex::new(TerminalManager::new()),
            jobs:           Mutex::new(JobRegistry::default()),
            plugin_logs:    Mutex::new(PluginLogBuffer::default()),
            // Seed the registry with runs persisted on disk (terminal/resumable
            // ones — Running/Pending get coerced to Failed by `load_persisted_runs`).
            // The internal counter is advanced past the highest recovered id.
            pipelines:      Mutex::new(crate::pipeline::registry_from_disk()),
            pipeline_cv:    Arc::new(std::sync::Condvar::new()),
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
            cloud_cancellations:    Mutex::new(HashMap::new()),
            cloud_pending_ops:      Mutex::new(HashMap::new()),
            brp:                    Mutex::new(BrpRegistry::default()),
            marketplace:            Mutex::new(MarketplaceRegistry::new()),
            boot_done:              Arc::new(AtomicBool::new(false)),
            boot_progress:          Arc::new(Mutex::new(None)),
        }
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

    // Deep-link + single-instance: always on in release, opt-in for debug
    // via the `deep-link-dev` Cargo feature (default-on for now; remove from
    // `default` features when you want it opt-in for dev).
    //
    // Single-instance must be the FIRST plugin: a duplicate launch (incl.
    // every `arbor://…` URL invocation) needs to short-circuit and forward
    // its argv to the running process before any other setup runs.  The
    // `deep-link` feature on `tauri-plugin-single-instance` makes the
    // forwarded argv flow straight into the deep-link plugin's `on_open_url`
    // callback registered in `setup()`.
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
        .manage(AppState::new())
        .setup(|app| {
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

            // Marketplace auto-refresh scheduler — long-lived background
            // task that polls the cache age and re-fetches when it ages
            // past the user's configured interval. Reads
            // `AppConfig.marketplace.refresh_hours` on every poll so
            // settings changes propagate without restart. Disabled when
            // the setting is `Some(0)` / `None`.
            crate::marketplace::scheduler::start(app.handle().clone());

            // Plugin loading moved to a background thread so the webview can
            // mount + render its boot-splash overlay BEFORE the (potentially
            // slow) discover → topo-sort → `load_plugin` pass blocks the
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
            std::thread::Builder::new()
                .name("arbor-plugin-boot".to_string())
                .spawn(move || {
                    use tauri::Emitter;
                    // The webview mounts AFTER `setup()` returns. Give it a
                    // moment to come up + register its event listeners before
                    // we start emitting `arbor://boot-progress` — otherwise
                    // early events (and possibly the terminal `boot-done`
                    // when load is fast) would be dropped on the floor and
                    // the splash would either skip the progress detail or
                    // stay stuck until the 10s fallback timeout.
                    std::thread::sleep(std::time::Duration::from_millis(250));

                    let state = handle_for_boot.state::<AppState>();
                    let mut host = state
                        .plugin_host
                        .lock()
                        .expect("plugin_host mutex poisoned during boot");
                    host.set_app_handle(handle_for_boot.clone());

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
                        "message": "Starting plugin schedulers…",
                    }));
                    host.start_all_schedulers();

                    mark_done(serde_json::json!({
                        "skipped": false,
                    }));
                })
                .expect("failed to spawn arbor-plugin-boot thread");

            // Periodic efficiency-mode re-apply.  When the app is unfocused we
            // re-scan child processes (WebView2 renderers and any subprocess
            // spawned since the last focus-change) and re-apply throttling.
            // Without this, renderers created while the app was already in the
            // background never receive EcoQoS and keep using full CPU.
            {
                let app_for_thread = app.handle().clone();
                std::thread::Builder::new()
                    .name("arbor-efficiency-periodic".to_string())
                    .spawn(move || {
                        loop {
                            std::thread::sleep(std::time::Duration::from_secs(30));
                            let state = app_for_thread.state::<AppState>();
                            let focused = state.app_focused.load(std::sync::atomic::Ordering::Relaxed);
                            if !focused {
                                let t = std::time::Instant::now();
                                crate::platform::set_efficiency_mode(true);
                                tracing::info!(
                                    target: "arbor::focus",
                                    "periodic re-apply set_efficiency_mode(true) took={}ms",
                                    t.elapsed().as_millis(),
                                );
                            }
                        }
                    })
                    .expect("failed to spawn efficiency-periodic thread");
            }

            // System tray — only in release builds
            #[cfg(not(debug_assertions))]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let show = MenuItem::with_id(app, "show", "Show Arbor", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;

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
                    #[cfg(not(debug_assertions))]
                    {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    #[cfg(debug_assertions)]
                    let _ = api;
                }
                tauri::WindowEvent::Focused(focused) => {
                    let focused = *focused;
                    let t_evt = std::time::Instant::now();
                    // Update the app-focused flag so focus-gated schedulers work correctly.
                    let state = window.app_handle().state::<AppState>();
                    state.app_focused.store(focused, std::sync::atomic::Ordering::Relaxed);
                    // Toggle OS-level power throttling (EcoQoS on Windows, nice/sched on
                    // Linux/macOS).  Handled here in the native window-event callback rather
                    // than via a frontend IPC call so that minimize / Alt-Tab / window-switch
                    // are all caught reliably via Win32 WM_SETFOCUS / WM_KILLFOCUS messages.
                    let t_eff = std::time::Instant::now();
                    crate::platform::set_efficiency_mode(!focused);
                    tracing::info!(
                        target: "arbor::focus",
                        "WindowEvent::Focused({focused}) handler total={}ms (set_efficiency_mode={}ms)",
                        t_evt.elapsed().as_millis(),
                        t_eff.elapsed().as_millis(),
                    );
                }
                tauri::WindowEvent::Resized(size) => {
                    // Windows reports minimize as a Resized event with width=0, height=0.
                    // Focused(false) alone doesn't always fire on minimize (depending on
                    // desktop/window-manager behavior), so we trigger efficiency mode
                    // from here too as a belt-and-braces catch.
                    if size.width == 0 && size.height == 0 {
                        let state = window.app_handle().state::<AppState>();
                        state.app_focused.store(false, std::sync::atomic::Ordering::Relaxed);
                        crate::platform::set_efficiency_mode(true);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Repo
            commands::repo_commands::open_repo,
            commands::repo_commands::close_repo,
            commands::repo_commands::get_repo_info,
            commands::repo_commands::check_is_git_repo,
            commands::repo_commands::get_git_identity,
            commands::repo_commands::init_repo,
            commands::repo_commands::clone_repo,
            commands::repo_commands::list_remote_branches_for_url,
            // Graph
            commands::graph_commands::get_graph,
            commands::graph_commands::get_graph_for_file,
            commands::graph_commands::get_repo_files,
            commands::graph_commands::get_files_last_commit,
            commands::graph_commands::start_file_meta_scan,
            commands::graph_commands::get_repo_file_tree,
            commands::graph_commands::get_commit_detail,
            commands::graph_commands::get_repo_fingerprint,
            commands::graph_commands::export_graph_svg,
            // Diff
            commands::diff_commands::get_commit_diff,
            commands::diff_commands::get_commit_diff_meta,
            commands::diff_commands::get_commit_file_diff,
            commands::diff_commands::get_workdir_diff,
            commands::diff_commands::get_workdir_diff_stream,
            commands::diff_commands::get_branch_diff,
            commands::diff_commands::get_file_at_commit,
            commands::diff_commands::get_file_blame,
            // Stage
            commands::stage_commands::stage_file,
            commands::stage_commands::unstage_file,
            commands::stage_commands::stage_all,
            commands::stage_commands::unstage_all,
            commands::stage_commands::stage_patch,
            commands::stage_commands::discard_file,
            commands::stage_commands::discard_all,
            commands::stage_commands::commit,
            commands::stage_commands::cherry_pick,
            commands::stage_commands::revert_commit,
            commands::stage_commands::get_git_commit_template,
            // Branches
            commands::branch_commands::get_status,
            commands::branch_commands::list_local_branches,
            commands::branch_commands::list_remote_branches,
            commands::branch_commands::list_tags,
            commands::branch_commands::get_nearest_tag,
            commands::branch_commands::create_branch,
            commands::branch_commands::delete_branch,
            commands::branch_commands::rename_branch,
            commands::branch_commands::checkout_branch,
            commands::branch_commands::checkout_branch_safe,
            commands::branch_commands::checkout_remote_as_local,
            commands::branch_commands::checkout_commit,
            commands::branch_commands::list_merged_branches,
            commands::branch_commands::delete_branches,
            commands::branch_commands::list_merged_remote_branches,
            commands::branch_commands::delete_remote_branches,
            commands::branch_commands::rename_remote_branch,
            // Remote
            commands::remote_commands::list_remotes,
            commands::remote_commands::fetch_remote,
            commands::remote_commands::push_branch,
            commands::remote_commands::pull_branch,
            // Stash
            commands::stash_commands::list_stashes,
            commands::stash_commands::list_graph_stash_refs,
            commands::stash_commands::stash_save,
            commands::stash_commands::stash_apply,
            commands::stash_commands::stash_pop,
            commands::stash_commands::stash_drop,
            commands::stash_commands::stash_rename,
            commands::stash_commands::abort_stash_apply,
            commands::stash_commands::force_stash_apply,
            commands::stash_commands::get_stash_file_content,
            commands::stash_commands::write_workdir_file,
            // Reset / Tags
            commands::reset_commands::reset_to_commit,
            commands::reset_commands::create_tag,
            commands::reset_commands::delete_tag,
            // Rebase
            commands::rebase_commands::get_rebase_todo,
            commands::rebase_commands::start_rebase,
            commands::rebase_commands::rebase_continue,
            commands::rebase_commands::rebase_abort,
            commands::rebase_commands::rebase_skip,
            commands::rebase_commands::get_rebase_state,
            // Search
            commands::search_commands::search_commits,
            // Auth — credentials
            commands::auth_commands::save_credential,
            commands::auth_commands::get_credential,
            commands::auth_commands::delete_credential,
            commands::auth_commands::save_default_credential,
            commands::auth_commands::has_default_credential,
            commands::auth_commands::delete_default_credential,
            // Auth — GitHub OAuth (Device Authorization Grant)
            commands::auth_commands::start_github_device_flow,
            commands::auth_commands::get_github_status,
            commands::auth_commands::get_github_user,
            commands::auth_commands::disconnect_github,
            commands::auth_commands::try_refresh_github_token,
            // Auth — GitLab OAuth (Authorization Code + PKCE)
            commands::auth_commands::start_gitlab_oauth,
            commands::auth_commands::get_gitlab_status,
            commands::auth_commands::get_gitlab_user,
            commands::auth_commands::disconnect_gitlab,
            commands::auth_commands::try_refresh_gitlab_token,
            // Auth — Linear OAuth (Authorization Code + PKCE)
            commands::auth_commands::start_linear_oauth,
            commands::auth_commands::get_linear_oauth_status,
            commands::auth_commands::disconnect_linear_oauth,
            commands::auth_commands::try_refresh_linear_token,
            // Auth — Jira OAuth (Authorization Code + PKCE) + Basic Auth
            commands::auth_commands::start_jira_oauth,
            commands::auth_commands::get_jira_oauth_status,
            commands::auth_commands::disconnect_jira,
            commands::auth_commands::try_refresh_jira_token,
            // Plugins
            commands::plugin_commands::list_plugins,
            commands::plugin_commands::get_plugin_directory,
            commands::plugin_commands::get_installed_plugin_path,
            commands::plugin_commands::get_plugins_enabled,
            commands::plugin_commands::set_plugins_enabled,
            commands::plugin_commands::reload_plugins,
            commands::plugin_commands::exec_hook,
            commands::plugin_commands::fire_plugin_action,
            commands::plugin_commands::enable_plugin,
            commands::plugin_commands::disable_plugin,
            commands::plugin_commands::plugin_enable_preview,
            commands::plugin_commands::plugin_disable_preview,
            commands::plugin_commands::delete_plugin,
            commands::plugin_commands::list_plugin_info,
            commands::plugin_commands::plugin_dep_graph,
            commands::plugin_commands::plugin_dependents,
            commands::plugin_commands::start_plugin_scheduler,
            commands::plugin_commands::stop_plugin_scheduler,
            commands::plugin_commands::plugin_settings_get,
            commands::plugin_commands::plugin_settings_set_all,
            commands::plugin_template_commands::export_plugin_template_to_path,
            commands::plugin_template_commands::import_plugin_zip,
            commands::plugin_template_commands::import_plugin_zip_from_path,
            // Session persistence
            commands::session_commands::save_session,
            commands::session_commands::load_session,
            // Workspaces
            commands::workspace_commands::list_workspaces,
            commands::workspace_commands::list_registry_repos,
            commands::workspace_commands::list_registry_with_roots,
            commands::workspace_commands::load_workspace_snapshot,
            commands::workspace_commands::save_workspace_snapshot,
            commands::workspace_commands::create_workspace,
            commands::workspace_commands::update_workspace,
            commands::workspace_commands::delete_workspace,
            commands::workspace_commands::reorder_workspaces,
            commands::workspace_commands::set_active_workspace,
            commands::workspace_commands::create_workspace_group,
            commands::workspace_commands::update_workspace_group,
            commands::workspace_commands::delete_workspace_group,
            commands::workspace_commands::reorder_workspace_groups,
            commands::workspace_commands::set_workspace_group,
            commands::workspace_commands::add_repo_to_workspace,
            commands::workspace_commands::remove_repo_from_workspace,
            commands::workspace_commands::move_repo_between_workspaces,
            commands::workspace_commands::register_repo_path,
            commands::workspace_commands::update_registry_repo,
            commands::workspace_commands::delete_registry_repo,
            commands::workspace_commands::export_workspace,
            commands::workspace_commands::import_workspace_preview,
            commands::workspace_commands::import_workspace_commit,
            commands::workspace_commands::workspace_health_scan,
            commands::workspace_commands::workspace_fetch_all,
            commands::workspace_commands::workspace_pull_all,
            commands::workspace_commands::workspace_tag_all,
            commands::workspace_commands::take_migration_report,
            // Per-repo config
            commands::config_commands::get_repo_config,
            commands::config_commands::set_repo_config,
            commands::config_commands::list_local_only_tags,
            commands::config_commands::mark_tag_local,
            commands::config_commands::mark_tag_pushed,
            // Recent repos (app-level config)
            commands::config_commands::get_recent_repos,
            commands::config_commands::add_recent_repo,
            // Graph config
            commands::config_commands::get_graph_config,
            commands::config_commands::set_graph_config,
            // Cache config
            commands::config_commands::get_cache_config,
            commands::config_commands::set_cache_config,
            commands::config_commands::evict_tab_cache,
            // Pipelines orchestrator config
            commands::config_commands::get_pipelines_config,
            commands::config_commands::set_pipelines_config,
            commands::config_commands::get_studio_settings,
            commands::config_commands::set_studio_settings,
            // Issues config
            commands::config_commands::get_issues_config,
            commands::config_commands::set_issues_config,
            // MR/PR Activity timeline defaults
            commands::config_commands::get_mr_config,
            commands::config_commands::set_mr_config,
            // Appearance preferences (window control style, …)
            commands::config_commands::get_appearance_config,
            commands::config_commands::set_appearance_config,
            // Activity bar config
            commands::config_commands::get_activity_bar_config,
            commands::config_commands::set_activity_bar_config,
            // Diff config (algorithm, context, full-file, virt threshold)
            commands::config_commands::get_diff_config,
            commands::config_commands::set_diff_config,
            // Missing-projects config (tombstone + locate)
            commands::config_commands::get_missing_projects_config,
            commands::config_commands::set_missing_projects_config,
            // OAuth client-id overrides
            commands::config_commands::get_oauth_overrides,
            commands::config_commands::set_oauth_overrides,
            commands::config_commands::get_oauth_defaults,
            // Terminal
            commands::terminal_commands::terminal_create,
            commands::terminal_commands::terminal_write,
            commands::terminal_commands::terminal_resize,
            commands::terminal_commands::terminal_close,
            commands::terminal_commands::terminal_list,
            commands::terminal_commands::terminal_default_shell,
            commands::terminal_commands::terminal_exec,
            commands::terminal_commands::list_builtin_shells,
            commands::terminal_commands::start_shell_detection,
            commands::terminal_commands::get_terminals_config,
            commands::terminal_commands::set_terminals_config,
            // Jobs
            commands::job_commands::list_jobs,
            commands::job_commands::get_job_output,
            commands::job_commands::cancel_job,
            commands::job_commands::running_job_count,
            commands::job_commands::dismiss_job,
            commands::job_commands::clear_finished_jobs,
            // Plugin logs (arbor.log.* ring buffer)
            commands::plugin_logs_commands::list_plugin_logs,
            commands::plugin_logs_commands::clear_plugin_logs,
            commands::plugin_logs_commands::clear_plugin_logs_by_pipeline,
            // App focus / active-tab state (used by focus-gated schedulers)
            commands::plugin_commands::set_app_focus,
            commands::plugin_commands::set_active_tab,
            // Boot state — polled by BootSplash to recover from listener-timing race
            commands::plugin_commands::get_boot_state,
            // Toolchains
            commands::plugin_commands::list_toolchains,
            commands::plugin_commands::add_toolchain,
            commands::plugin_commands::remove_toolchain,
            commands::plugin_commands::set_active_toolchain,
            commands::plugin_commands::detect_toolchains,
            // Cross-plugin contribution model — tree snapshots and custom
            // icons are read through the unified registry, no parallel IPC.
            commands::plugin_commands::list_plugin_contributions,
            commands::plugin_commands::list_contribution_points,
            // Container model (Phase 2 — ContributableModal)
            commands::plugin_commands::list_containers,
            commands::plugin_commands::get_container,
            // Git Flow
            commands::gitflow_commands::get_gitflow_config,
            commands::gitflow_commands::get_gitflow_global_config,
            commands::gitflow_commands::set_gitflow_global_config,
            commands::gitflow_commands::set_gitflow_repo_config,
            commands::gitflow_commands::clear_gitflow_repo_config,
            commands::gitflow_commands::gitflow_get_status,
            commands::gitflow_commands::gitflow_init,
            commands::gitflow_commands::gitflow_init_create_main,
            commands::gitflow_commands::gitflow_feature_start,
            commands::gitflow_commands::gitflow_feature_finish,
            commands::gitflow_commands::gitflow_release_start,
            commands::gitflow_commands::gitflow_release_finish,
            commands::gitflow_commands::gitflow_hotfix_start,
            commands::gitflow_commands::gitflow_hotfix_finish,
            commands::gitflow_commands::has_gitflow_repo_override,
            // Open in browser
            commands::remote_commands::open_in_browser,
            // Submodules
            commands::submodule_commands::list_submodules,
            commands::submodule_commands::submodule_fetch,
            commands::submodule_commands::submodule_pull,
            commands::submodule_commands::submodule_push,
            commands::submodule_commands::submodule_checkout,
            commands::submodule_commands::submodule_list_branches,
            commands::submodule_commands::update_submodule,
            commands::submodule_commands::update_all_submodules,
            // Pipelines (plugin-defined)
            commands::pipeline_commands::list_pipeline_defs,
            commands::pipeline_commands::list_pipeline_runs,
            commands::pipeline_commands::get_pipeline_run,
            commands::pipeline_commands::run_pipeline,
            commands::pipeline_commands::request_pipeline_run,
            commands::pipeline_commands::cancel_pipeline_run,
            commands::pipeline_commands::resume_pipeline_run,
            commands::pipeline_commands::discard_pipeline_run,
            commands::pipeline_commands::is_pipeline_locked,
            // Pipelines (CI/CD — GitHub Actions + GitLab CI)
            commands::pipeline_commands::get_ci_provider,
            commands::pipeline_commands::fetch_ci_runs,
            commands::pipeline_commands::fetch_mr_ci_runs,
            commands::pipeline_commands::fetch_ci_jobs,
            commands::pipeline_commands::list_ci_workflows,
            commands::pipeline_commands::create_ci_pipeline,
            commands::pipeline_commands::retrigger_ci_run,
            // Security dashboard (GitLab Phase 1; GitHub Phase 6)
            commands::security_commands::supports_security,
            commands::security_commands::fetch_security_summary,
            commands::security_commands::fetch_security_findings,
            commands::security_commands::export_security_report,
            // App-level metadata (About modal)
            commands::app_commands::get_app_info,
            // Filesystem browser
            commands::fs_commands::fs_read_dir,
            commands::fs_commands::list_fs_roots,
            commands::fs_commands::fs_create_dir,
            commands::fs_commands::fs_create_file,
            commands::fs_commands::fs_write_text_file,
            commands::fs_commands::fs_read_text_file,
            commands::fs_commands::fs_rename,
            commands::fs_commands::fs_delete,
            // Avatar resolution via GitProvider (GitHub + GitLab)
            commands::avatar_commands::resolve_avatar_for_email,
            // Merge Requests / Pull Requests (GitHub + GitLab)
            commands::mr_commands::list_mrs,
            commands::mr_commands::get_mr_detail,
            commands::mr_commands::create_mr,
            commands::mr_commands::get_mr_capabilities,
            commands::mr_commands::probe_mr_feature,
            commands::mr_commands::disable_mr_auto_merge,
            commands::mr_commands::merge_mr,
            commands::mr_commands::close_mr,
            commands::mr_commands::reopen_mr,
            commands::mr_commands::mark_mr_ready,
            commands::mr_commands::add_mr_comment,
            commands::mr_commands::get_mr_files,
            commands::mr_commands::get_mr_commits,
            commands::mr_commands::get_mr_commit_diff,
            commands::mr_commands::get_merged_mr_hints,
            commands::mr_commands::mr_start_conflict_resolution,
            // Issues / Linear
            commands::issues_commands::linear_get_auth_status,
            commands::issues_commands::linear_save_token,
            commands::issues_commands::linear_logout,
            commands::issues_commands::linear_search_issues,
            commands::issues_commands::linear_get_issue,
            commands::issues_commands::linear_get_filter_options,
            commands::issues_commands::linear_transition_issue,
            commands::issues_commands::linear_assign_issue,
            commands::issues_commands::linear_add_comment,
            commands::issues_commands::linear_create_issue,
            commands::issues_commands::linear_branch_name_for_issue,
            // Issues / Jira
            commands::issues_commands::jira_get_auth_status,
            commands::issues_commands::jira_save_basic_auth,
            commands::issues_commands::jira_logout,
            commands::issues_commands::jira_search_issues,
            commands::issues_commands::jira_get_issue,
            commands::issues_commands::jira_get_filter_options,
            commands::issues_commands::jira_transition_issue,
            commands::issues_commands::jira_assign_issue,
            commands::issues_commands::jira_add_comment,
            commands::issues_commands::jira_create_issue,
            commands::issues_commands::jira_branch_name_for_issue,
            commands::issues_commands::jira_download_attachment,
            // Merge conflict resolution
            commands::merge_commands::merge_branch,
            commands::merge_commands::get_conflict_content,
            commands::merge_commands::get_conflict_presence,
            commands::merge_commands::remove_conflict_file,
            commands::merge_commands::resolve_conflict,
            commands::merge_commands::resolve_stash_conflict,
            commands::merge_commands::complete_merge,
            commands::merge_commands::abort_merge,
            commands::merge_commands::get_merge_message,
            // Ticket links (commit ↔ ticket association)
            commands::ticket_commands::get_commit_ticket_links,
            commands::ticket_commands::add_ticket_link,
            commands::ticket_commands::remove_ticket_link,
            commands::ticket_commands::get_ticket_link_config,
            commands::ticket_commands::set_ticket_link_repo_config,
            commands::ticket_commands::validate_ticket_regex,
            commands::ticket_commands::check_notes_push_config,
            commands::ticket_commands::find_commits_for_ticket,
            // Git Notes
            commands::notes_commands::list_commit_notes,
            commands::notes_commands::save_commit_note,
            commands::notes_commands::delete_commit_note,
            commands::notes_commands::check_note_remote_status,
            commands::notes_commands::push_note_namespace,
            // Worktrees
            commands::worktree_commands::list_worktrees,
            commands::worktree_commands::add_worktree,
            commands::worktree_commands::remove_worktree,
            commands::worktree_commands::detect_project_type,
            commands::worktree_commands::start_ide_detection,
            commands::worktree_commands::open_in_ide,
            commands::worktree_commands::get_ide_config,
            commands::worktree_commands::set_ide_config,
            commands::worktree_commands::get_repo_ide,
            commands::worktree_commands::set_repo_ide,
            // Reflog
            commands::reflog_commands::get_reflog,
            // Recovery journal (pre-destructive snapshots)
            commands::recovery_commands::list_recovery_entries,
            commands::recovery_commands::preview_recovery_restore,
            commands::recovery_commands::restore_recovery_entry,
            commands::recovery_commands::delete_recovery_entry,
            commands::config_commands::get_recovery_config,
            commands::config_commands::set_recovery_config,
            // Bisect
            commands::bisect_commands::bisect_start,
            commands::bisect_commands::bisect_mark,
            commands::bisect_commands::bisect_reset,
            commands::bisect_commands::get_bisect_state,
            commands::bisect_commands::bisect_undo_last_mark,
            commands::bisect_commands::list_bisect_sessions,
            commands::bisect_commands::save_bisect_session,
            commands::bisect_commands::save_bisect_result,
            commands::bisect_commands::resume_bisect_session,
            commands::bisect_commands::rename_bisect_session,
            commands::bisect_commands::delete_bisect_session,
            // Remote repository browser
            commands::repo_browser_commands::rb_list_accounts,
            commands::repo_browser_commands::rb_list_repos,
            commands::repo_browser_commands::rb_browse_tree,
            commands::repo_browser_commands::rb_get_file_content,
            commands::repo_browser_commands::rb_download_file,
            // Repository statistics (background computation, result via event)
            commands::stats_commands::compute_repo_stats,
            commands::stats_commands::export_repo_stats,
            // Theme
            commands::theme_commands::list_custom_themes,
            commands::theme_commands::get_active_theme_id,
            commands::theme_commands::set_active_theme_id,
            commands::theme_commands::save_custom_theme,
            commands::theme_commands::delete_custom_theme,
            // Branding (in-memory, plugin-driven) + theme-changed hook bridge
            commands::branding_commands::get_branding,
            commands::branding_commands::notify_theme_changed,
            // Linked Worktrees (cross-project sync)
            commands::linked_worktree_commands::list_worktree_links,
            commands::linked_worktree_commands::get_worktree_link,
            commands::linked_worktree_commands::get_worktree_link_for_repo,
            commands::linked_worktree_commands::create_worktree_link,
            commands::linked_worktree_commands::delete_worktree_link,
            commands::linked_worktree_commands::rename_worktree_link,
            commands::linked_worktree_commands::add_worktree_link_member,
            commands::linked_worktree_commands::remove_worktree_link_member,
            commands::linked_worktree_commands::set_worktree_link_sync_enabled,
            commands::linked_worktree_commands::set_worktree_link_member_sync_enabled,
            commands::linked_worktree_commands::add_alias_group,
            commands::linked_worktree_commands::update_alias_group,
            commands::linked_worktree_commands::remove_alias_group,
            // Git CLI executable detection / configuration
            commands::git_cli_commands::get_git_status,
            commands::git_cli_commands::redetect_git,
            commands::git_cli_commands::verify_git_path,
            commands::git_cli_commands::set_git_path,
            commands::git_cli_commands::download_portable_git,
            commands::git_cli_commands::cancel_git_download,
            // Deep-link router (arbor:// URLs)
            commands::deep_link_commands::find_repo_by_remote_url,
            commands::deep_link_commands::deep_link_ready,
            commands::deep_link_commands::get_deep_link_config,
            commands::deep_link_commands::set_deep_link_config,
            // Missing-repo tombstone + locate
            commands::missing_commands::validate_repo_path,
            commands::missing_commands::validate_repo_paths,
            commands::missing_commands::relocate_repo,
            commands::missing_commands::report_repo_missing,
            commands::missing_commands::remove_recent_repo,
            commands::missing_commands::cleanup_missing_recent_repos,
            // Studio Multi-Format backbone (FROZEN F17) — one set of
            // commands dispatched via `AppState.studio_registry` to a
            // per-format `StudioFormatBackend` impl. RON + JSON are
            // registered today; TOML / YAML / .properties join later phases.
            crate::studio::format::commands::studio_list_formats,
            crate::studio::format::commands::studio_describe,
            crate::studio::format::commands::studio_parse,
            crate::studio::format::commands::studio_close,
            crate::studio::format::commands::studio_get_encoding,
            crate::studio::format::commands::studio_set_text,
            crate::studio::format::commands::studio_get_root,
            crate::studio::format::commands::studio_get_children,
            crate::studio::format::commands::studio_get_value,
            crate::studio::format::commands::studio_query,
            crate::studio::format::commands::studio_raw_original,
            crate::studio::format::commands::studio_raw_current,
            crate::studio::format::commands::studio_format,
            crate::studio::format::commands::studio_to_json,
            crate::studio::format::commands::studio_from_json,
            crate::studio::format::commands::studio_save,
            crate::studio::format::commands::studio_source_path,
            crate::studio::format::commands::studio_diff,
            crate::studio::format::commands::studio_tree_diff,
            crate::studio::format::commands::studio_undo,
            crate::studio::format::commands::studio_redo,
            crate::studio::format::commands::studio_history_state,
            crate::studio::format::commands::studio_snapshot,
            crate::studio::format::commands::studio_list_files,
            crate::studio::format::commands::studio_schema_probe,
            crate::studio::format::commands::studio_schema_load,
            crate::studio::format::commands::studio_apply_mutation,
            crate::studio::format::commands::studio_strip_features,
            crate::studio::format::commands::studio_get_indent,
            crate::studio::format::commands::studio_set_indent,
            crate::studio::format::commands::studio_schema_view_source,
            crate::studio::format::commands::studio_rename_preview,
            crate::studio::format::commands::studio_rename_apply,
            crate::studio::format::commands::studio_bulk_edit_preview,
            crate::studio::format::commands::studio_bulk_edit_apply,
            crate::studio::format::commands::studio_yaml_to_properties,
            crate::studio::format::commands::studio_properties_to_yaml,
            // studio sidebar — project-wide .ron/.json/.toml index.
            commands::studio_commands::studio_scan_repo,
            commands::studio_commands::studio_scan_cross_refs,
            commands::studio_commands::studio_find_usages,
            commands::studio_commands::studio_scan_broken_refs,
            commands::studio_commands::studio_add_external,
            commands::studio_commands::studio_remove_external,
            commands::studio_commands::studio_get_config,
            commands::studio_commands::studio_toggle_exclude,
            commands::studio_commands::studio_bind_schema,
            commands::studio_commands::studio_unbind_schema,
            commands::studio_commands::studio_toggle_reference_field,
            commands::studio_commands::studio_refresh_index,
            // cloud-storage plugin — opendal-backed GCS (S3/Azure ready in backend).
            commands::cloud_commands::cloud_secret_set,
            commands::cloud_commands::cloud_secret_exists,
            commands::cloud_commands::cloud_secret_delete,
            commands::cloud_commands::cloud_test_connection,
            commands::cloud_commands::cloud_list,
            commands::cloud_commands::cloud_list_stream,
            commands::cloud_commands::cloud_search_stream,
            commands::cloud_commands::cloud_stat,
            commands::cloud_commands::cloud_delete,
            commands::cloud_commands::cloud_copy,
            commands::cloud_commands::cloud_download,
            commands::cloud_commands::cloud_upload,
            commands::cloud_commands::cloud_sync,
            commands::cloud_commands::cloud_download_many,
            commands::cloud_commands::cloud_concat_files,
            commands::cloud_commands::cloud_is_cancelled,
            commands::cloud_commands::cloud_cancel,
            commands::cloud_commands::cloud_report_progress,
            commands::cloud_commands::cloud_report_done,
            commands::cloud_commands::cloud_gcs_oauth_start,
            // Bevy Remote Protocol (Phase 1.0 — read-only HTTP)
            commands::brp_commands::brp_connect,
            commands::brp_commands::brp_disconnect,
            commands::brp_commands::brp_status,
            commands::brp_commands::brp_call,
            // Marketplace (Phase 1 — in-memory stub)
            commands::marketplace_commands::marketplace_list_installed,
            commands::marketplace_commands::marketplace_fetch_registry,
            commands::marketplace_commands::marketplace_refresh_registry,
            commands::marketplace_commands::marketplace_installed_plugin_names,
            commands::marketplace_commands::marketplace_remove_custom_source,
            commands::marketplace_commands::marketplace_get_refresh_hours,
            commands::marketplace_commands::marketplace_set_refresh_hours,
            commands::marketplace_commands::marketplace_get_poll_minutes,
            commands::marketplace_commands::marketplace_set_poll_minutes,
            commands::marketplace_commands::marketplace_install_plugin,
            commands::marketplace_commands::marketplace_uninstall_plugin,
            commands::marketplace_commands::marketplace_set_plugin_enabled,
            commands::marketplace_commands::marketplace_install_theme,
            commands::marketplace_commands::marketplace_uninstall_theme,
            commands::marketplace_commands::marketplace_add_custom_source,
        ])
    .run(tauri::generate_context!())
        .expect("error while running arbor");
}
