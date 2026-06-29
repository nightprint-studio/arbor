//! `packs` domain — the two **read handlers** for downloadable sample packs.
//!
//! The pack **read surface** — the [`Pack`] descriptor table, install status, the
//! cheap (no-decode) instrument listing, the per-profile **active** allow-list
//! ([`active_packs`]), and the lazy [`load_subset_into`] the audio thread decodes
//! through — lives in [`merula_core::packs`] (the audio substrate decodes through
//! it, so it moved into the core). This module re-exports that surface so the
//! sibling domains (`packs_download` / `sounds` / the eval validator) keep
//! addressing it as `crate::packs::*`, and keeps only the two
//! `#[arbor_rpc::handler]`s here:
//!
//!   * `merula_packs` — list every pack with its install status.
//!   * `merula_pack_set_active` — toggle a pack's active state in the allow-list.
//!
//! The job-tracked download / reindex / delete plumbing stays in
//! `crate::packs_download`.

// Re-export the whole read surface so `crate::packs::{Pack, Layout, PackStatus,
// InstallManifest, pack, pack_dir, read_manifest, status_of, installed_ids,
// installed_instrument_names, instrument_pack_map, list, list_instruments_into,
// load_subset_into, active_packs, GM_SF2_URL, ...}` keep resolving for the sibling
// be modules that import them.
pub use merula_core::packs::*;

use merula_core::config;
use merula_core::prelude::MerulaState;

// ── Handlers ─────────────────────────────────────────────────────────────────

/// List every downloadable sample pack with its current install status.
#[arbor_rpc::handler]
fn merula_packs(_ctx: &MerulaState) -> Result<Vec<PackStatus>, String> {
    Ok(list(&config::load()))
}

/// Toggle a pack's **active** state in the per-profile allow-list. Inactive packs
/// stay installed (pack management still sees them) but their instruments are
/// hidden from playback, the eval validator, and the sound bank. Seeds the
/// allow-list from the currently-installed packs on the first toggle, so turning
/// one pack off keeps every other installed pack on.
#[arbor_rpc::handler]
fn merula_pack_set_active(_ctx: &MerulaState, pack_id: String, active: bool) -> Result<(), String> {
    let cfg = config::load();
    let installed_ids = installed_ids(&cfg);
    active_packs::set_active(&pack_id, active, &installed_ids)
}
