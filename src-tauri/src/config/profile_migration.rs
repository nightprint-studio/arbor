//! One-shot relocation of the pre-profiles **flat** on-disk layout into the
//! active profile's product buckets (`docs/profiles-and-product-config.md`).
//!
//! Phase 2 split the monolithic `config.toml`; this handles the *satellite*
//! files that were never inside it — `workspaces.json`, `repos.json`,
//! `session.json`, `workspace-state/`, `graph_columns.toml`,
//! `linked_worktrees.toml`, `pipeline_runs/`. Their readers/writers now resolve
//! under `arbor/profiles/<active>/corvus/`, so without this move an upgraded
//! install would silently come up with empty workspaces / repos / tabs.
//!
//! Idempotent and non-destructive: each entry moves only when the legacy source
//! exists and the new destination does not, so a second boot — or a partially
//! migrated state — is a no-op. Runs early in `AppState::new()`, before anything
//! reads these paths. Plugin-area files relocate in a later phase.

use std::path::Path;

use arbor_core::prelude::{arbor_config_path, product_path, profile_plugins_dir, PRODUCT_CORVUS};

/// Corvus product satellite entries (files and directories) that lived flat
/// under `arbor/` before profiles. Same basename on both sides — only the
/// parent folder changes.
const CORVUS_SATELLITES: &[&str] = &[
    "workspaces.json",
    "repos.json",
    "session.json",
    "workspace-state",
    "graph_columns.toml",
    "linked_worktrees.toml",
    "pipeline_runs",
];

/// Plugin-area entries that lived flat under `arbor/` before profiles, as
/// `(legacy flat name, sub-path under the profile's `plugins/` area)`. Mirrors
/// the destinations the plugin crates now resolve to (`profile_plugins_dir()`).
/// The old `-dev`-suffixed twins belong to the `dev` profile and are not
/// migrated — a fresh `dev` profile simply re-fetches its caches.
const PLUGIN_AREA: &[(&str, &str)] = &[
    ("plugins",                    "installed"),
    ("plugin_states.json",         "plugin_states.json"),
    ("plugin_data",                "plugin_data"),
    ("toolchains",                 "toolchains"),
    ("themes",                     "themes"),
    ("marketplace_cache.json",     "marketplace_cache.json"),
    ("marketplace_custom.json",    "marketplace_custom.json"),
    ("marketplace_installed.json", "marketplace_installed.json"),
    ("user_registry.toml",         "user_registry.toml"),
    ("marketplace_plugins",        "marketplace_plugins"),
];

/// Relocate the pre-profiles flat files into the **default** profile's buckets:
/// corvus satellites → `corvus/`, plugin-area state → `plugins/`. The flat files
/// conceptually belong to the default profile, so the move is gated to it — a
/// non-default profile must never absorb them (and in practice this only ever
/// runs on the first boot, which is always `default`). Best-effort: a failure on
/// one entry is logged and skipped, never fatal to boot.
pub fn migrate_flat_satellites_to_active_profile() {
    if arbor_core::prelude::active_profile() != arbor_core::prelude::DEFAULT_PROFILE {
        return;
    }
    for name in CORVUS_SATELLITES {
        move_if_absent(&arbor_config_path(name), &product_path(PRODUCT_CORVUS, name));
    }
    let plugins = profile_plugins_dir();
    for (flat, sub) in PLUGIN_AREA {
        move_if_absent(&arbor_config_path(flat), &plugins.join(sub));
    }
}

/// Move `old` → `new` only when `old` exists and `new` does not. Within
/// `%APPDATA%` this is a same-volume rename (atomic, works for files and
/// directories alike); a failure leaves the source in place for a retry next
/// boot rather than risking a partial copy.
fn move_if_absent(old: &Path, new: &Path) {
    if !old.exists() || new.exists() {
        return;
    }
    if let Some(parent) = new.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("profile migration: mkdir {parent:?} failed: {e}");
            return;
        }
    }
    if let Err(e) = std::fs::rename(old, new) {
        tracing::warn!("profile migration: move {old:?} -> {new:?} failed: {e}");
    }
}
