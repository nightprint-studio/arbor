//! One-shot relocation of the pre-profiles **flat** on-disk layout into the
//! active profile's product buckets (`docs/profiles-and-product-config.md`).
//!
//! Phase 2 split the monolithic `config.toml`; this handles the *satellite*
//! files that were never inside it — `workspaces.json`, `repos.json`,
//! `session.json`, `workspace-state/`, `linked_worktrees.toml`,
//! `pipeline_runs/`. Their readers/writers now resolve
//! under `arbor/profiles/<active>/corvus/`, so without this move an upgraded
//! install would silently come up with empty workspaces / repos / tabs.
//!
//! Idempotent and non-destructive: each entry moves only when the legacy source
//! exists and the new destination does not, so a second boot — or a partially
//! migrated state — is a no-op. Runs early in `AppState::new()`, before anything
//! reads these paths. Plugin-area files relocate in a later phase.

use std::path::Path;

use arbor_core::prelude::{
    arbor_config_path, arbor_profile_path, product_path, profile_plugins_dir, PRODUCT_CORVUS,
};

/// Corvus product satellite entries (files and directories) that lived flat
/// under `arbor/` before profiles. Same basename on both sides — only the
/// parent folder changes.
const CORVUS_SATELLITES: &[&str] = &[
    "workspaces.json",
    "repos.json",
    "session.json",
    "workspace-state",
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

/// Keys that USED to persist to `corvus/config.toml` (everything-not-generic
/// landed there) but are now **shell-owned** and belong in `profile.toml`: the
/// `git` executable override, integrated-terminal prefs, activity-bar layout,
/// IDE-launcher prefs, and the recent-repos list. corvus-be owns
/// `corvus/config.toml` now, so on upgrade these must be lifted out of it.
const RELOCATED_TO_PROFILE: &[&str] = &["git", "terminals", "activity_bar", "ide", "recent_repos"];

/// One-shot: lift the now-shell-owned keys out of corvus-be's `corvus/config.toml`
/// into the active profile's `profile.toml`. Without this an upgraded install
/// would lose those settings (the shell stopped reading the corvus file) and the
/// stale copies would shadow fresh writes if corvus-be (the file's sole owner)
/// rewrote it. Idempotent: once the keys are gone from the corvus file it's a
/// no-op. Best-effort — any parse/write failure is logged and skipped, never
/// fatal to boot. Runs per active profile (each has its own corvus file).
pub fn migrate_generic_keys_out_of_corvus_config() {
    let corvus_p = product_path(PRODUCT_CORVUS, "config.toml");
    let Ok(text) = std::fs::read_to_string(&corvus_p) else { return };
    let Ok(mut corvus_tbl) = text.parse::<toml::Table>() else { return };

    let mut lifted = toml::Table::new();
    for key in RELOCATED_TO_PROFILE {
        if let Some(v) = corvus_tbl.remove(*key) {
            lifted.insert((*key).to_string(), v);
        }
    }
    if lifted.is_empty() {
        return; // already migrated, or a fresh install with no legacy keys
    }

    // Merge into profile.toml. On upgrade these keys never lived there, so the
    // `or_insert` only guards against a (re-run) collision.
    let profile_p = arbor_profile_path("profile.toml");
    let mut profile_tbl = std::fs::read_to_string(&profile_p)
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .unwrap_or_default();
    for (k, v) in lifted {
        profile_tbl.entry(k).or_insert(v);
    }

    // Preserve first (profile.toml), then strip from the corvus file — never the
    // other way round, so a mid-migration crash can't lose the keys.
    if let Err(e) = write_table(&profile_p, &profile_tbl) {
        tracing::warn!("profile migration: write {profile_p:?} failed: {e}");
        return;
    }
    if let Err(e) = write_table(&corvus_p, &corvus_tbl) {
        tracing::warn!("profile migration: rewrite {corvus_p:?} failed: {e}");
    }
}

fn write_table(path: &Path, tbl: &toml::Table) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(tbl)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, content)
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
