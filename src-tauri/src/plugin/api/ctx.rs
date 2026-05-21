//! Shared registration context.
//!
//! Every namespace installer takes `&ApiCtx` and clones the fields it needs
//! into its closures. The struct keeps the `register()` orchestrator's
//! signature short and stops every namespace module from re-listing the
//! same dozen captures.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::plugin::contribution::ContributionRegistry;
use crate::plugin::runtime::{
    AccessLevel, GitLevel, PluginPermissions, ScheduleRegistry,
    TerminalLevel, TimerCancels, TimerCounter,
};
use crate::plugin::tree::{IconRegistry, TreeStore};

/// Snapshot of everything a namespace closure may capture. All fields are
/// cheap to clone (`Arc<…>` for registries, `String`/`Vec<String>` for the
/// rest) so destructuring at the top of each `install_*` is fine.
pub(crate) struct ApiCtx {
    pub plugin_name: String,
    pub plugin_dir: PathBuf,
    pub arbor_api: u32,

    pub app_handle: Option<tauri::AppHandle>,

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
    #[allow(dead_code)]
    pub provider_write:      bool,
    pub toolchain_read:      bool,
    pub toolchain_write:     bool,
    pub service_export:      bool,
    pub service_call:        bool,
    pub settings_read_others: bool,

    // ── Shared registries ────────────────────────────────────────────────
    pub contributions: ContributionRegistry,
    pub tree_store:    TreeStore,
    pub icon_registry: IconRegistry,

    /// Live enable flag — closures consult this to no-op cleanly when the
    /// plugin is disabled mid-call.
    pub enabled: Arc<AtomicBool>,
}

impl ApiCtx {
    pub fn from_register_args(
        plugin_name: String,
        plugin_dir: PathBuf,
        arbor_api: u32,
        app_handle: Option<tauri::AppHandle>,
        timer_cancels: TimerCancels,
        timer_counter: TimerCounter,
        schedules: ScheduleRegistry,
        scheduler_enabled: bool,
        permissions: PluginPermissions,
        contributions: ContributionRegistry,
        tree_store: TreeStore,
        icon_registry: IconRegistry,
        enabled: Arc<AtomicBool>,
    ) -> Self {
        // env_read is consumed by sandbox.rs (harden_os_table); not used here.
        let PluginPermissions {
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
        } = permissions;

        Self {
            plugin_name,
            plugin_dir,
            arbor_api,
            app_handle,
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
            contributions,
            tree_store,
            icon_registry,
            enabled,
        }
    }
}
