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

/// The directory captures are actually written to, with the default resolved.
///
/// `output.dir` is empty by default and *means* "wherever this OS keeps videos", so
/// the config alone can't answer "where do my captures go" — and a frontend that
/// guesses ends up showing a Windows path on a Mac, which is what it used to do.
#[arbor_rpc::handler]
fn output_dir(_state: &TytoState) -> Result<String, String> {
    Ok(crate::capture::output_dir().to_string_lossy().to_string())
}

/// Persist the typed product tyto config (pretty TOML), creating the dir if needed.
///
/// Returns the config as it was actually written — normalized, so the caller learns
/// the derived values (today the bitrate the quality preset implies) instead of
/// keeping its own copy of the table to guess them with.
#[arbor_rpc::handler]
fn set_tyto_config(_state: &TytoState, config: TytoConfig) -> Result<TytoConfig, String> {
    save(&config)
}
