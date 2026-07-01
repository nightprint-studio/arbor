//! Profile management commands (keep-shell): CRUD over `arbor/profiles/<name>/`
//! and the active-profile switch. Plain `#[tauri::command]`s — profile
//! management is a host/platform concern, not a product-backend domain, and
//! `switch_profile` needs the `AppHandle` to relaunch. Called via `invoke` from
//! `src/lib/ipc/profiles.ts`. See `docs/profiles-and-product-config.md`.

use crate::error::AppError;
use crate::profile;

/// Every profile on disk, `default` first.
#[tauri::command]
pub fn list_profiles() -> Vec<String> {
    profile::list()
}

/// The currently active profile name.
#[tauri::command]
pub fn get_active_profile() -> String {
    arbor_core::prelude::active_profile()
}

/// Create a new (empty) profile. It loads built-in defaults until populated.
#[tauri::command]
pub fn create_profile(name: String) -> Result<(), AppError> {
    profile::create(&name)
}

/// Clone a profile: recursively copy `src`'s folder (settings, plugins, repos)
/// into a fresh profile named `new`. The clone starts inactive.
#[tauri::command]
pub fn clone_profile(src: String, new: String) -> Result<(), AppError> {
    profile::clone(&src, &new)
}

/// Rename a profile; if it was active, the pointer follows it.
#[tauri::command]
pub fn rename_profile(old: String, new: String) -> Result<(), AppError> {
    profile::rename(&old, &new)
}

/// Delete a profile (not the active one, not the last remaining one).
#[tauri::command]
pub fn delete_profile(name: String) -> Result<(), AppError> {
    profile::delete(&name)
}

/// Switch the active profile **live** (no relaunch): persist the pointer, flip
/// the in-process profile cell, re-resolve every per-profile backend cache and
/// the plugin host against the new profile, then broadcast
/// `arbor://profile-switched` so each window reloads its frontend stores. A
/// no-op (returns `Ok`) when already on the target profile.
#[tauri::command]
pub fn switch_profile(
    state: tauri::State<'_, crate::AppState>,
    name: String,
) -> Result<(), AppError> {
    if !profile::exists(&name) {
        return Err(AppError::Other(format!("no such profile: {name}")));
    }
    if name == arbor_core::prelude::active_profile() {
        return Ok(());
    }
    // Persist the pointer + flip the cell so every profile-scoped path now
    // resolves to the target before we reload anything.
    profile::write_pointer(&name)?;
    arbor_core::prelude::set_active_profile(&name);
    state.reload_for_active_profile();
    // Reload the plugin host so the new profile's plugin set is picked up.
    if let Err(e) = crate::ipc::platform::plugin::reload_runtime(&state) {
        tracing::warn!("profile switch: plugin reload failed: {e}");
    }
    // Every window (this one included) reloads its stores on this event.
    state.emit("arbor://profile-switched", serde_json::json!({ "profile": name }));
    Ok(())
}
