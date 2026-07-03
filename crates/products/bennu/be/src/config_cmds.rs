//! `config_cmds` domain — the `get/set_bennu_config` handlers.
//!
//! The typed **product** bennu configuration (per-profile `…/bennu/config.toml`) —
//! the [`BennuConfig`] type plus its `load` / `save` — lives in
//! [`bennu_core::config`]. Only the two `#[arbor_rpc::handler]`s stay here, calling
//! back into it. `_state` is unused — the path is self-resolved — but the handler
//! signature requires the ctx.

use bennu_core::config::{load, save, BennuConfig};
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
    save(&config)
}
