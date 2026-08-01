//! Shared registration context.
//!
//! Every namespace installer takes `&ApiCtx` and clones the fields it needs
//! into its closures. The struct keeps the [`register`](super::register)
//! orchestrator's signature short and stops every namespace module from
//! re-listing the same dozen captures.
//!
//! ## Tauri vs plugin-core
//!
//! `ApiCtx` lives here, in plugin-core — it knows nothing about Tauri. The
//! Tauri-shell ns/* that still need a real `tauri::AppHandle` recover it from
//! `app_ctx` via an `as_any()`-based downcast (see the `ApiCtxExt` shim in
//! `src-tauri/src/plugin/api/ctx.rs`). Once every ns has migrated to its own
//! domain crate (PR #6+), the `AppCtx` capability surface should grow to
//! cover whatever the ns needed and the downcast goes away.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::AtomicBool;

use arbor_core::prelude::AppCtx;
use arbor_plugin_types::prelude::{
    AccessLevel, GitLevel, Permissions, ScheduleRegistry, TerminalLevel,
};

use crate::contribution::ContributionRegistry;
use crate::runtime::host::PluginHost;
use crate::runtime::loaded::{TimerCancels, TimerCounter};
use crate::sandbox::ApiInstallParams;
use crate::tree::{IconRegistry, TreeStore};

/// Snapshot of everything a namespace closure may capture. All fields are
/// cheap to clone (`Arc<…>` for registries, `String`/`Vec<String>` for the
/// rest) so destructuring at the top of each `install_*` is fine.
pub struct ApiCtx {
    pub plugin_name: String,
    pub plugin_dir: PathBuf,
    pub arbor_api: u32,

    /// Product id of the host loading this plugin — the namespace an
    /// unqualified `arbor.events.on("commit", …)` resolves against (**D9**).
    /// `None` when the host never bound a product (headless / unit-test runs),
    /// in which case subscriptions are registered exactly as written.
    pub product: Option<String>,

    /// Host capability handle. `None` in headless / test runs that don't
    /// route through a real shell. Tauri-shell ns/* downcast this to a
    /// concrete `TauriAppCtx` via `as_any()` (see the `ApiCtxExt` shim in
    /// src-tauri) until they migrate into domain crates.
    pub app_ctx: Option<Arc<dyn AppCtx>>,

    /// Weak self-reference of the owning [`PluginHost`]. Captured by `arbor.*`
    /// closures that need to fire hooks / invoke services back into the
    /// runtime from a background thread (`events.emit`, `timer.after/every`,
    /// `service.call`, `http.get` response delivery, …). `None` when the
    /// plugin was loaded outside a `PluginHost` (e.g. unit tests calling the
    /// standalone `load_plugin` helper).
    pub host_weak: Option<Weak<Mutex<PluginHost>>>,

    pub timer_cancels: TimerCancels,
    pub timer_counter: TimerCounter,
    pub schedules: ScheduleRegistry,
    pub scheduler_enabled: bool,

    // ── Permissions (snapshot at load time) ──────────────────────────────
    pub network_perm:        Vec<String>,
    pub fs_perm:             AccessLevel,
    pub fs_scope:            Vec<String>,
    pub git_read:            bool,
    pub git_write:           bool,
    pub terminal_perm:       TerminalLevel,
    pub terminal_scope:      Vec<String>,
    pub issues_read:         bool,
    pub issues_write:        bool,
    pub provider_read:       bool,
    pub provider_write:      bool,
    pub toolchain_read:      bool,
    pub toolchain_write:     bool,
    pub service_export:      bool,
    pub service_call:        bool,
    pub settings_read_others: bool,
    pub command_invoke:      bool,

    // ── Shared registries ────────────────────────────────────────────────
    pub contributions: ContributionRegistry,
    pub tree_store:    TreeStore,
    pub icon_registry: IconRegistry,

    /// Live enable flag — closures consult this to no-op cleanly when the
    /// plugin is disabled mid-call.
    pub enabled: Arc<AtomicBool>,
}

impl ApiCtx {
    /// Build an `ApiCtx` from the parameters that the sandbox builder hands
    /// to a [`LuaApiInstaller`](crate::sandbox::LuaApiInstaller). Centralises
    /// the permission-snapshot destructuring so every consumer of
    /// `ApiInstallParams` sees the same typed view.
    pub fn from_install_params(params: ApiInstallParams) -> Self {
        let ApiInstallParams {
            plugin_name,
            plugin_dir,
            arbor_api,
            product,
            app_ctx,
            host_weak,
            timer_cancels,
            timer_counter,
            schedules,
            scheduler_enabled,
            permissions,
            contributions,
            tree_store,
            icon_registry,
            enabled,
        } = params;

        // env_read is consumed by sandbox.rs (harden_os_table); not used here.
        let Permissions {
            network: network_perm,
            fs: fs_perm,
            fs_scope,
            git: git_perm,
            terminal: terminal_perm,
            terminal_scope,
            env_read: _env_read,
            issues: issues_perm,
            provider: provider_perm,
            toolchain: toolchain_perm,
            service_export,
            service_call,
            settings_read_others,
            command_invoke,
            // Free-form catch-all for crate-contributed permission keys
            // (validated by PluginRegistry::validate_manifest at boot, not
            // consumed by the typed ApiCtx fields).
            ext: _,
        } = permissions;

        Self {
            plugin_name,
            plugin_dir,
            arbor_api,
            product,
            app_ctx,
            host_weak,
            timer_cancels,
            timer_counter,
            schedules,
            scheduler_enabled,
            network_perm,
            fs_perm,
            fs_scope,
            git_read:            git_perm        >= GitLevel::Read,
            git_write:           git_perm        >= GitLevel::Write,
            terminal_perm,
            terminal_scope,
            issues_read:         issues_perm     >= AccessLevel::Read,
            issues_write:        issues_perm     >= AccessLevel::Write,
            provider_read:       provider_perm   >= AccessLevel::Read,
            provider_write:      provider_perm   >= AccessLevel::Write,
            toolchain_read:      toolchain_perm  >= AccessLevel::Read,
            toolchain_write:     toolchain_perm  >= AccessLevel::Write,
            service_export,
            service_call,
            settings_read_others,
            command_invoke,
            contributions,
            tree_store,
            icon_registry,
            enabled,
        }
    }
}
