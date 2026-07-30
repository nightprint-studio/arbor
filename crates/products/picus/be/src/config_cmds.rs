//! `config_cmds` domain — what Picus remembers between sessions.
//!
//! The typed **product** picus configuration (per-profile `…/picus/config.toml`) —
//! the [`PicusConfig`] type plus its `load` / `save` — lives in
//! [`picus_core::config`], and the unsaved query tabs in
//! [`picus_core::scratch`]. Only the `#[arbor_rpc::handler]`s stay here, calling
//! back into them. `_state` is unused throughout — every path is self-resolved from
//! the active profile — but the handler signature requires the ctx.
//!
//! Two files rather than one, for the reason connections are also their own: a
//! settings file is a preference somebody may hand-edit, and a scratchpad is
//! multi-line SQL rewritten on every keystroke. A corrupt one must not take the
//! other down with it.

use picus_core::config::{load, save, PicusConfig};
use picus_core::prelude::{load_scratch, save_scratch, PicusState, Scratch};

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

// ── The scratchpad ───────────────────────────────────────────────────────────

/// The unsaved query tabs, as they were left.
///
/// Read once when the window opens. An empty scratchpad is the ordinary answer on a
/// first run and is not an error.
#[arbor_rpc::handler]
fn picus_load_scratch(_state: &PicusState) -> Result<Scratch, String> {
    Ok(load_scratch())
}

/// Remember the unsaved query tabs.
///
/// Called debounced from the interface while the user types, and once more when the
/// window is closing. Idempotent and whole-file — see [`picus_core::prelude::save_scratch`]
/// for why it is not patched per tab.
#[arbor_rpc::handler]
fn picus_save_scratch(_state: &PicusState, scratch: Scratch) -> Result<(), String> {
    save_scratch(&scratch)
}
