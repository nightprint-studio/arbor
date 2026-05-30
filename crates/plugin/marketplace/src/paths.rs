//! Canonical filesystem locations for marketplace state.
//!
//! Centralizes the four state files / two state directories the marketplace
//! reads and writes so the `if cfg!(debug_assertions) { "...-dev" } else
//! { "..." }` pattern lives in exactly one place — adding a new state file
//! is a one-line addition here, not a copy-paste of the cfg dance.

use std::path::PathBuf;

use arbor_core::prelude::arbor_config_path;

/// Cached snapshot of the last successful community catalog fetch.
/// TTL-checked on read (see [`crate::cache::TTL_SECS`]).
pub fn community_cache_file() -> PathBuf {
    arbor_config_path(switch("marketplace_cache.json", "marketplace_cache-dev.json"))
}

/// Cached resolved metadata for every user-added custom source. No TTL —
/// refreshed every time the modal opens.
pub fn custom_cache_file() -> PathBuf {
    arbor_config_path(switch("marketplace_custom.json", "marketplace_custom-dev.json"))
}

/// Source-of-truth ledger for which marketplace entries are currently
/// installed (plus their enable state).
pub fn installs_file() -> PathBuf {
    arbor_config_path(switch(
        "marketplace_installed.json",
        "marketplace_installed-dev.json",
    ))
}

/// User-added source pointers (composite key `repo + subpath`).
pub fn user_registry_file() -> PathBuf {
    arbor_config_path(switch("user_registry.toml", "user_registry-dev.toml"))
}

/// Directory marketplace-installed plugins land in. Kept distinct from the
/// host's dev plugin directory so the two pools never collide on disk —
/// dev wins at load time, but the marketplace install still has its own
/// home so an upgrade / reinstall can rewrite it atomically.
pub fn plugins_dir() -> PathBuf {
    arbor_config_path(switch("marketplace_plugins", "marketplace_plugins-dev"))
}

/// Directory holding both user-created custom themes (saved via the
/// SettingsPanel) and marketplace-installed theme JSONs. Theme loading
/// scans this directory at app boot.
pub fn themes_dir() -> PathBuf {
    arbor_config_path(switch("themes", "themes-dev"))
}

fn switch<'a>(release: &'a str, debug: &'a str) -> &'a str {
    if cfg!(debug_assertions) { debug } else { release }
}
