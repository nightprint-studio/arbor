//! `config_cmds` domain — the `get/set_picus_config` handlers.
//!
//! The typed **product** picus configuration (per-profile `…/picus/config.toml`) —
//! the [`PicusConfig`] type plus its `load` / `save` — lives in
//! [`picus_core::config`]. Only the two `#[arbor_rpc::handler]`s stay here, calling
//! back into it. `_state` is unused — the path is self-resolved — but the handler
//! signature requires the ctx.

use picus_core::config::{load, save, PicusConfig};
use picus_core::prelude::PicusState;

/// Read the typed product picus config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_picus_config(_state: &PicusState) -> Result<PicusConfig, String> {
    Ok(load())
}

/// Persist the typed product picus config (pretty TOML), creating the dir if needed.
#[arbor_rpc::handler]
fn set_picus_config(_state: &PicusState, config: PicusConfig) -> Result<(), String> {
    save(&config)
}
