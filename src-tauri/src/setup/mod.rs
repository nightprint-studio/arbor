//! Tauri app construction + first-run wiring, split out of `lib.rs`.
//!
//! - [`init_tracing`] installs the log subscriber.
//! - [`build_builder`] assembles the `tauri::Builder` (plugins, managed state,
//!   the `setup` hook and the window-event handler) — everything but the
//!   `invoke_handler` list (which lives in the `invoke_handlers!` macro) and the
//!   final `.run()`.
//! - [`run`] is the `setup` hook body: it wires the backend, schedulers, deep
//!   links, the plugin-boot thread and the tray, each delegated to a submodule.

mod boot;
mod deep_link;
mod scheduler;
mod tray;

use tauri::Manager;

use crate::AppState;

/// Install the tracing subscriber (honours `RUST_LOG`, defaults to `info`).
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

/// Assemble the Tauri builder up to (but excluding) the invoke handler + run.
/// The caller chains `.invoke_handler(invoke_handlers!())` and `.run(...)`.
pub fn build_builder() -> tauri::Builder<tauri::Wry> {
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
    // the deep-link plugin's `on_open_url` callback registered in `run()`.
    #[cfg(any(not(debug_assertions), feature = "deep-link-dev"))]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    crate::window::show_and_focus(&w);
                }
            }))
            .plugin(tauri_plugin_deep_link::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // OS-global shortcut (Ctrl+Shift+E) — dedicated File Explorer window.
        // The handler only reacts on key-down for our one registered combo;
        // the combo itself is registered in `run()` below.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state() == ShortcutState::Pressed {
                        if let Some(sc) = crate::window::explorer::current_explorer_shortcut() {
                            if shortcut == &sc {
                                crate::window::explorer::open_or_focus(app);
                            }
                        }
                    }
                })
                .build(),
        )
        .manage(AppState::new())
        .manage(crate::window::explorer::PendingReveals::default())
        .manage(crate::window::explorer::ExplorerClipboard::default())
        .manage(crate::window::explorer::DragOverlayText::default())
        .manage(crate::merula::MerulaState::default())
        .setup(run)
        .on_window_event(|window, event| crate::window::events::handle(window, event))
}

/// The `setup` hook body — runs once, before the event loop starts (so before
/// any command routes). Each concern is delegated to a submodule.
fn run(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // One-shot: merula's storage is now SPLIT — config/state are profile-scoped
    // (`profiles/<active>/merula`) while the multi-GB sample banks live in the
    // global `data/merula` shared across profiles. Fan the legacy top-level
    // sibling (`%APPDATA%\merula`, or pre-rename `…\nemus`) out into both
    // destinations before anything reads it. Idempotent + non-destructive, so it
    // converges across re-runs. Runs after the active profile is seeded (in
    // `AppState::new`, built before this setup hook).
    crate::merula::config::migrate_legacy_dirs();

    // Seed the Corvus backend state (in-process `corvus-be`) + the Model-D IPC
    // router into AppState. Both need the `AppHandle` that `AppState::new()`
    // predates. Must run before any command routes — safe here because commands
    // only fire once `Builder::run()` enters its event loop, after `run()`
    // returns.
    app.state::<AppState>().wire_backend(app.handle());

    // NB: `corvus-be` is no longer spawned here. It starts lazily the first time
    // the Corvus window opens (`window::corvus::open_corvus_window` →
    // `ipc::ensure_corvus_be`), which also pushes the config to it — so the
    // launcher and the non-git product windows never pay for a git backend they
    // don't use.

    // Wire the `arbor-cloud` crate against AppState: registers the Google OAuth
    // refresher and publishes the `Arc<dyn CloudHost>` into Tauri state. Must
    // run after the event sink is wired above (the host stores the sink for
    // `emit_event`).
    crate::cloud::install(&app.handle());

    // Park the launcher (main window) bottom-right, JetBrains-Toolbox-style.
    crate::window::placement::place_launcher_bottom_right(app.handle());

    // Register the configured OS-global File-Explorer shortcut (opt-in; no-op
    // when disabled or unset). The press handler is wired on the plugin builder.
    #[cfg(desktop)]
    crate::window::explorer::register_configured(app.handle());

    // Register the `arbor://` URI scheme + deep-link routing (warm + cold start).
    #[cfg(all(desktop, any(not(debug_assertions), feature = "deep-link-dev")))]
    deep_link::register(app);

    // Shared trigger engine (`arbor-scheduler`) + PluginHost app-context wiring.
    scheduler::wire(app);

    // Marketplace auto-refresh — one entry in the shared engine.
    crate::marketplace::scheduler::install(app.handle().clone());

    // Plugin loading on a background thread (so the boot-splash renders first);
    // blocks until the boot thread has the plugin_host lock, so every
    // plugin-touching IPC queues behind boot.
    boot::spawn(app);

    // Efficiency-mode driver (off-thread EcoQoS scan) + taskbar-icon refresh
    // (Windows + WebView2 drops the small HICON on wake).
    crate::efficiency::init();
    crate::taskbar_icon_refresh::install(app.handle());

    // System tray — release builds only.
    #[cfg(not(debug_assertions))]
    tray::install(app)?;

    Ok(())
}
