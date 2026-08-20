//! `config_cmds` domain — the `get/set_bennu_config` handlers.
//!
//! The typed **product** bennu configuration (per-profile `…/bennu/config.toml`) —
//! the [`BennuConfig`] type plus its `load` / `save` — lives in
//! [`bennu_core::config`]. Only the two `#[arbor_rpc::handler]`s stay here, calling
//! back into it. `_state` is unused — the path is self-resolved — but the handler
//! signature requires the ctx.

use bennu_core::config::{
    load, load_workspaces, save, save_workspaces, BennuConfig, BennuWorkspaces, OnboardingConfig,
};
use bennu_core::prelude::BennuState;

/// Read the typed product bennu config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_bennu_config(_state: &BennuState) -> Result<BennuConfig, String> {
    Ok(load())
}

/// Persist the typed product bennu config (pretty TOML), creating the dir if needed. Also
/// re-seeds the classpath's extra JDK search dirs so a newly-added `jdk_paths` entry takes
/// effect on the next index build without a restart.
#[arbor_rpc::handler]
fn set_bennu_config(_state: &BennuState, config: BennuConfig) -> Result<(), String> {
    bennu_classpath::prelude::set_extra_jdk_homes(
        config.jdk_paths.iter().map(std::path::PathBuf::from).collect(),
    );
    // A language server Bennu had recorded as "not installed" is not looked for again until
    // something says it might be there now (see `LspRegistry::forget_missing`). Saving the
    // config is one of those things: it is how an executable path is pinned by hand, and how
    // a disabled server is turned back on.
    crate::lsp_registry::LspRegistry::global().forget_missing();
    save(&config)
}

/// Read the persisted **workspace store** (every named workspace + which is active + each
/// project's session). Empty store on a missing/corrupt file — the window then opens with nothing
/// restored. A pre-named-workspaces file is migrated on read (see `load_workspaces`).
#[arbor_rpc::handler]
fn get_bennu_workspaces(_state: &BennuState) -> Result<BennuWorkspaces, String> {
    Ok(load_workspaces())
}

/// Persist the **workspace store** (the FE writes it debounced on tab/project/switch/CRUD changes)
/// so the next launch reopens the active workspace where the user left off.
#[arbor_rpc::handler]
fn set_bennu_workspaces(_state: &BennuState, workspaces: BennuWorkspaces) -> Result<(), String> {
    save_workspaces(&workspaces)
}

// ── onboarding ──
// Its own pair rather than a field of the big config the settings modal round-trips: the tour
// finishing is one boolean written once, and routing it through `set_bennu_config` would mean
// the tour serialising every setting the user has — including any a settings dialog open at
// the same moment is in the middle of editing.

/// Whether the user has been through Bennu's welcome tour.
#[arbor_rpc::handler]
fn get_bennu_onboarding(_state: &BennuState) -> Result<OnboardingConfig, String> {
    Ok(load().onboarding)
}

/// Record that the tour was finished or skipped.
#[arbor_rpc::handler]
fn set_bennu_onboarding(_state: &BennuState, config: OnboardingConfig) -> Result<(), String> {
    let mut current = load();
    current.onboarding = config;
    save(&current)
}
