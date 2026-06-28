use std::path::PathBuf;

/// Directory holding per-workspace tab snapshots (`workspace-state/<id>.json`).
///
/// The snapshots themselves are owned and read/written by corvus-be; the shell
/// only resolves this profile-aware path and pushes it to the backend via
/// `sync_config` (corvus-be can't compute the active-profile path itself).
pub fn snapshot_dir() -> PathBuf {
    arbor_core::prelude::product_path(arbor_core::prelude::PRODUCT_CORVUS, "workspace-state")
}
