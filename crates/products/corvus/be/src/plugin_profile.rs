//! Live profile-switch support for the plugin host.
//!
//! corvus-be resolves its plugin scan roots — both of the profile's pools, `installed/` and
//! `marketplace_plugins/` — through the process-global active-profile cell, seeded once at
//! boot (`init_active_profile`). A **live** profile switch in the launcher must update that
//! cell here, before the shell triggers a `reload_plugins`, or corvus-be would reload the
//! previous profile's plugin set. The launcher owns the active profile and pushes the new
//! name through this `__`-prefixed method (corvus-be can't observe the switch otherwise —
//! workspaces/config arrive as already-resolved paths).
//!
//! The roots themselves need no recomputing: they are derived from the cell on every
//! discovery, so moving the cell is the whole of the switch.

use corvus_core::prelude::CorvusState;

/// Point this process at `profile`, so a following `reload_plugins` scans the new
/// profile's packages. Does not reload by itself — the shell calls `reload_plugins`
/// right after.
#[arbor_rpc::handler]
fn __set_plugin_profile(_state: &CorvusState, profile: String) -> Result<(), String> {
    arbor_core::prelude::set_active_profile(&profile);
    Ok(())
}
