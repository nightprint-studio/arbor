//! `config_cmds` domain — the `get/set_tyto_config` handlers.
//!
//! The typed **product** tyto configuration (per-profile `…/tyto/config.toml`) —
//! the [`TytoConfig`] type plus its `load` / `save` — lives in
//! [`tyto_core::config`]. Only the two `#[arbor_rpc::handler]`s stay here, calling
//! back into it. `_state` is unused — the path is self-resolved — but the handler
//! signature requires the ctx.

use tyto_core::config::{load, save, TytoConfig};
use tyto_core::prelude::TytoState;

/// Read the typed product tyto config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_tyto_config(_state: &TytoState) -> Result<TytoConfig, String> {
    Ok(load())
}

/// Persist the typed product tyto config (pretty TOML), creating the dir if needed.
#[arbor_rpc::handler]
fn set_tyto_config(_state: &TytoState, config: TytoConfig) -> Result<(), String> {
    save(&config)
}
