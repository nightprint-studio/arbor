//! Profile management — CRUD over `arbor/profiles/<name>/` plus the global
//! `active-profile` pointer (`docs/profiles-and-product-config.md`).
//!
//! A profile is an isolated environment (own settings, plugins, repos). This
//! module is the filesystem layer; the Tauri commands in
//! `crate::commands::profile_commands` wrap it. **Switching** a profile is a
//! process relaunch (see `switch_profile`): every per-profile cache in
//! `AppState` is built once in `AppState::new()`, so restarting is the robust
//! way to re-resolve them all against the new profile rather than hot-swapping
//! a dozen mutexes.

use std::fs;

use arbor_core::prelude::{
    active_profile, active_profile_pointer_path, is_valid_profile_name, profile_dir_for,
    profiles_root, set_active_profile, DEFAULT_PROFILE,
};

use crate::error::{AppError, Result};

fn invalid(name: &str) -> AppError {
    AppError::Other(format!("invalid profile name: {name:?}"))
}

/// Every profile present on disk, plus the implicit [`DEFAULT_PROFILE`], sorted
/// alphabetically with `default` pinned first.
pub fn list() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(profiles_root()) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    if is_valid_profile_name(name) {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }
    if !names.iter().any(|n| n == DEFAULT_PROFILE) {
        names.push(DEFAULT_PROFILE.to_string());
    }
    names.sort();
    names.dedup();
    if let Some(pos) = names.iter().position(|n| n == DEFAULT_PROFILE) {
        let d = names.remove(pos);
        names.insert(0, d);
    }
    names
}

/// Whether a profile exists (the default always does, even before its folder is
/// materialized by a first write).
pub fn exists(name: &str) -> bool {
    name == DEFAULT_PROFILE || profile_dir_for(name).is_dir()
}

/// Create an empty profile folder. Its config/plugins/repos materialize on
/// first write once it becomes active — a fresh profile loads built-in
/// defaults.
pub fn create(name: &str) -> Result<()> {
    let name = name.trim();
    if !is_valid_profile_name(name) {
        return Err(invalid(name));
    }
    let dir = profile_dir_for(name);
    if dir.exists() {
        return Err(AppError::Other(format!("profile already exists: {name}")));
    }
    fs::create_dir_all(&dir).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// Rename a profile's folder. The `default` profile can't be renamed (it's the
/// migration target + fallback). If the active profile is renamed, follow it in
/// both the in-process cell and the on-disk pointer.
pub fn rename(old: &str, new: &str) -> Result<()> {
    let new = new.trim();
    if !is_valid_profile_name(new) {
        return Err(invalid(new));
    }
    if old == DEFAULT_PROFILE {
        return Err(AppError::Other("cannot rename the default profile".into()));
    }
    let (from, to) = (profile_dir_for(old), profile_dir_for(new));
    if !from.is_dir() {
        return Err(AppError::Other(format!("no such profile: {old}")));
    }
    if to.exists() {
        return Err(AppError::Other(format!("profile already exists: {new}")));
    }
    fs::rename(&from, &to).map_err(|e| AppError::Other(e.to_string()))?;
    if active_profile() == old {
        set_active_profile(new);
        write_pointer(new)?;
    }
    Ok(())
}

/// Delete a profile's folder and everything in it. Guards against stranding the
/// user: the active profile and the last remaining profile can't be deleted.
pub fn delete(name: &str) -> Result<()> {
    if name == active_profile() {
        return Err(AppError::Other("cannot delete the active profile".into()));
    }
    let dir = profile_dir_for(name);
    if !dir.is_dir() {
        return Err(AppError::Other(format!("no such profile: {name}")));
    }
    if list().len() <= 1 {
        return Err(AppError::Other("cannot delete the only profile".into()));
    }
    fs::remove_dir_all(&dir).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// Persist the `active-profile` pointer (read at boot by
/// `init_active_profile`). Does not itself reload state — the caller relaunches.
pub fn write_pointer(name: &str) -> Result<()> {
    if !is_valid_profile_name(name) {
        return Err(invalid(name));
    }
    let path = active_profile_pointer_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    fs::write(&path, name).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}
