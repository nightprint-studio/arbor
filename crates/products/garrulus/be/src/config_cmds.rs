//! `config_cmds` domain — the `get/set_garrulus_config` handlers.
//!
//! The typed **global** garrulus configuration (per-profile
//! `…/garrulus/config.toml`) — the `GarrulusConfig` type plus its `load` / `save`
//! — lives in `garrulus_core::config`. Only the two `#[arbor_rpc::handler]`s stay
//! here, calling back into it. `_state` is unused — the path is self-resolved —
//! but the handler signature requires the ctx.

use garrulus_core::prelude::{load_config, save_config, GarrulusConfig, GarrulusState};

use crate::probe;

/// Read the typed global garrulus config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_garrulus_config(_state: &GarrulusState) -> Result<GarrulusConfig, String> {
    Ok(load_config())
}

/// Persist the typed global garrulus config (pretty TOML), creating the dir if
/// needed, and apply the parts of it something is already running on.
///
/// The sync probe's cadence is the one such part today: it is read at
/// registration, so without this a user changing it would see nothing happen
/// until the next launch — a setting that appears not to work.
#[arbor_rpc::handler]
fn set_garrulus_config(_state: &GarrulusState, config: GarrulusConfig) -> Result<(), String> {
    save_config(&config)?;
    probe::reconfigure();
    Ok(())
}
