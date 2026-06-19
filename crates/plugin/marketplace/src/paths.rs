//! Canonical filesystem locations for marketplace state.
//!
//! All of it lives under the active profile's plugin area
//! (`arbor/profiles/<profile>/plugins/`). The old `if cfg!(debug_assertions)
//! { "...-dev" }` filename dance is gone — debug builds get their isolation
//! from the `dev` profile (`arbor-core`'s build default), not a suffix. See
//! `docs/profiles-and-product-config.md`.

use std::path::PathBuf;

use arbor_core::prelude::profile_plugins_dir;

/// Cached snapshot of the last successful community catalog fetch.
/// TTL-checked on read (see [`crate::cache::TTL_SECS`]).
pub fn community_cache_file() -> PathBuf {
    profile_plugins_dir().join("marketplace_cache.json")
}

/// Cached resolved metadata for every user-added custom source. No TTL —
/// refreshed every time the modal opens.
pub fn custom_cache_file() -> PathBuf {
    profile_plugins_dir().join("marketplace_custom.json")
}

/// Source-of-truth ledger for which marketplace entries are currently
/// installed (plus their enable state).
pub fn installs_file() -> PathBuf {
    profile_plugins_dir().join("marketplace_installed.json")
}

/// User-added source pointers (composite key `repo + subpath`).
pub fn user_registry_file() -> PathBuf {
    profile_plugins_dir().join("user_registry.toml")
}

/// Directory marketplace-installed plugins land in. Kept distinct from the
/// host's `installed/` plugin directory so the two pools never collide on disk —
/// the host dir wins at load time, but the marketplace install still has its own
/// home so an upgrade / reinstall can rewrite it atomically.
pub fn plugins_dir() -> PathBuf {
    profile_plugins_dir().join("marketplace_plugins")
}

/// Directory holding both user-created custom themes (saved via the
/// SettingsPanel) and marketplace-installed theme JSONs. Theme loading
/// scans this directory at app boot.
pub fn themes_dir() -> PathBuf {
    profile_plugins_dir().join("themes")
}
