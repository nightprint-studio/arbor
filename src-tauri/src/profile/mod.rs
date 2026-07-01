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

/// Clone a profile: recursively copy `src`'s folder into a fresh `dst` folder,
/// carrying over its settings/plugins/repos. The clone starts inactive; the
/// caller switches to it explicitly. Mirrors [`create`]'s validation on the new
/// name, and fails if `dst` already exists or `src` is missing.
pub fn clone(src: &str, dst: &str) -> Result<()> {
    let dst = dst.trim();
    if !is_valid_profile_name(dst) {
        return Err(invalid(dst));
    }
    let from = profile_dir_for(src);
    // The default profile's folder may not be materialized yet — a clone of it
    // is effectively a fresh empty profile (built-in defaults), so allow it.
    if src != DEFAULT_PROFILE && !from.is_dir() {
        return Err(AppError::Other(format!("no such profile: {src}")));
    }
    let to = profile_dir_for(dst);
    if to.exists() {
        return Err(AppError::Other(format!("profile already exists: {dst}")));
    }
    if from.is_dir() {
        copy_dir_recursive(&from, &to).map_err(|e| AppError::Other(e.to_string()))?;
    } else {
        // Source folder not materialized (default before first write): create an
        // empty clone that loads built-in defaults, matching `create`.
        fs::create_dir_all(&to).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

/// Recursively copy `src` into `dst` (both directories). Creates `dst` and any
/// nested subdirectories; copies file contents verbatim. Pure filesystem logic,
/// covered by the unit tests below.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
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

#[cfg(test)]
mod tests {
    use super::copy_dir_recursive;
    use std::fs;

    /// Build a small nested tree under a unique temp dir; the caller removes it.
    fn unique_temp() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("arbor-profile-test-{}", uuid_like()));
        p
    }

    // Avoid pulling a uuid dep into a test — a monotonic-ish token is enough to
    // keep parallel test runs from colliding on the temp path.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{nanos:x}")
    }

    #[test]
    fn copies_nested_tree_verbatim() {
        let root = unique_temp();
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("top.txt"), b"top").unwrap();
        fs::write(src.join("nested").join("deep.toml"), b"deep=1").unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        assert_eq!(fs::read(dst.join("top.txt")).unwrap(), b"top");
        assert_eq!(
            fs::read(dst.join("nested").join("deep.toml")).unwrap(),
            b"deep=1"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn creates_destination_for_empty_source() {
        let root = unique_temp();
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_dir(&dst).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&root);
    }
}
