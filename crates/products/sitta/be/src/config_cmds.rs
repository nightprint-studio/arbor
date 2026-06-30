//! `config_cmds` domain — the `get/set_sitta_config` handlers.
//!
//! The typed **global** sitta configuration (per-profile `…/sitta/config.toml`) —
//! the [`SittaConfig`] type plus its `load` / `save` — lives in
//! [`sitta_core::config`]. Only the two `#[arbor_rpc::handler]`s stay here, in
//! sitta-be, calling back into [`sitta_core::config::load`] / [`save`]. `_state` is
//! unused — the path is self-resolved — but the handler signature requires the ctx.

use sitta_core::config::{load, save, SittaConfig};
use sitta_core::prelude::SittaState;

/// Read the typed global sitta config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_sitta_config(_state: &SittaState) -> Result<SittaConfig, String> {
    Ok(load())
}

/// Persist the typed global sitta config (pretty TOML), creating the dir if needed.
#[arbor_rpc::handler]
fn set_sitta_config(_state: &SittaState, config: SittaConfig) -> Result<(), String> {
    save(&config)
}
