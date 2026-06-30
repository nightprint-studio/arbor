//! Live profile-switch support for the plugin host.
//!
//! corvus-be resolves its plugin scan roots — the host's built-in `plugin_dir()`
//! (`…/plugins/installed`) and the marketplace extra root
//! (`…/plugins/marketplace_plugins`) — through the process-global active-profile
//! cell, seeded once at boot (`init_active_profile`). A **live** profile switch in
//! the launcher must update that cell here and recompute the extra root *before*
//! the shell triggers a `reload_plugins`, or corvus-be would reload the previous
//! profile's plugin set. The launcher owns the active profile and pushes the new
//! name through this `__`-prefixed method (corvus-be can't observe the switch
//! otherwise — workspaces/config arrive as already-resolved paths).

use corvus_core::prelude::CorvusState;

/// Point this process at `profile` and recompute the marketplace plugin root so a
/// following `reload_plugins` scans the new profile's installed set. Does not
/// reload by itself — the shell calls `reload_plugins` right after.
#[arbor_rpc::handler]
fn __set_plugin_profile(_state: &CorvusState, profile: String) -> Result<(), String> {
    arbor_core::prelude::set_active_profile(&profile);
    crate::host_handle::host()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .set_extra_plugin_roots(vec![arbor_core::prelude::marketplace_plugins_dir()]);
    Ok(())
}
