//! `config_cmds` domain — the `get/set_merula_config` handlers.
//!
//! The typed **global** merula configuration (`%APPDATA%\merula\config.toml`,
//! per-profile) — the [`MerulaConfig`] type plus its `load` / `save` — lives in
//! [`merula_core::config`] (the audio substrate reads it at session start, so it
//! moved into the core with the rest of the state substrate). Only the two
//! `#[arbor_rpc::handler]`s stay here, in merula-be, calling back into
//! [`merula_core::config::load`] / [`save`].

use merula_core::config::{load, save, MerulaConfig};
use merula_core::prelude::MerulaState;

// ── Handlers ─────────────────────────────────────────────────────────────────
// Plain `#[arbor_rpc::handler]`s with a `&MerulaState` context; the method +
// param names match the frontend payloads. `_state` is unused — the path is
// self-resolved — but the handler signature requires the ctx.

/// Read the typed global merula config (defaults on a missing/corrupt file).
#[arbor_rpc::handler]
fn get_merula_config(_state: &MerulaState) -> Result<MerulaConfig, String> {
    Ok(load())
}

/// Persist the typed global merula config (pretty TOML), creating the dir if needed.
#[arbor_rpc::handler]
fn set_merula_config(_state: &MerulaState, config: MerulaConfig) -> Result<(), String> {
    save(&config)
}
