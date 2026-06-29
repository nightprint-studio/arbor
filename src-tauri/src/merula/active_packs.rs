//! Per-profile **active packs** selection: a small allow-list that filters which
//! *installed* sample packs are actually usable by the audio session, the eval
//! validator, and the sound bank. Pack management (download / delete / reindex)
//! always sees every pack — only the *consumption* paths honour this list.
//!
//! Identity is the stable [`Pack::id`](super::packs::Pack) string
//! (`vsco`/`vcsl`/`dirt-samples`/`drum-machines`/`gm`).
//!
//! Migration-safety: an **absent or unparseable** file means *all packs active*
//! ([`active_set`] returns `None`), so an existing install keeps every pack until
//! the user explicitly toggles one off (which seeds the file from the currently
//! installed set, see [`set_active`]).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// The per-profile active-pack allow-list, persisted as `active_packs.toml`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ActivePacks {
    /// Pack ids the user has marked active. Absent file = all active.
    #[serde(default)]
    ids: Vec<String>,
}

/// Per-profile path of the active-packs file.
fn config_path() -> std::path::PathBuf {
    arbor_core::prelude::merula_config_path("active_packs.toml")
}

/// The set of active pack ids, or `None` when the file is absent / unparseable.
///
/// `None` is the migration-safe "all packs active" sentinel — never an error.
/// `Some(set)` is the explicit allow-list the user has chosen.
pub fn active_set() -> Option<HashSet<String>> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    let parsed: ActivePacks = toml::from_str(&text).ok()?;
    Some(parsed.ids.into_iter().collect())
}

/// Whether `id` is active given an [`active_set`] result. `None` (no file) means
/// every pack is active.
pub fn is_active(active: &Option<HashSet<String>>, id: &str) -> bool {
    match active {
        None => true,
        Some(s) => s.contains(id),
    }
}

/// Persist the active-pack allow-list (no BOM). Creates the parent dir.
pub fn save(ids: &[String]) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let payload = ActivePacks { ids: ids.to_vec() };
    let text = toml::to_string_pretty(&payload).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Toggle one pack's active state and persist the result.
///
/// When the file is absent (all-active), this **seeds** the list from
/// `installed_ids` first, so toggling a single pack off keeps every other
/// installed pack on (instead of suddenly making only one pack active).
pub fn set_active(id: &str, on: bool, installed_ids: &[String]) -> Result<(), String> {
    let mut ids: Vec<String> = match active_set() {
        Some(set) => set.into_iter().collect(),
        // No file yet: every installed pack is currently active — start from that
        // set so a single toggle-off doesn't drop the rest.
        None => installed_ids.to_vec(),
    };
    let present = ids.iter().any(|x| x == id);
    if on && !present {
        ids.push(id.to_string());
    } else if !on && present {
        ids.retain(|x| x != id);
    }
    save(&ids)
}

/// Auto-activate a freshly-installed pack so the user never loses access to a
/// pack they just downloaded.
///
/// Only mutates an **existing** allow-list (appending `id` if missing). When the
/// file is absent we're already in all-active mode, so there's nothing to do.
pub fn on_pack_installed(id: &str, installed_ids: &[String]) {
    let Some(set) = active_set() else {
        return; // all-active: a new install is already usable.
    };
    if set.contains(id) {
        return;
    }
    let mut ids: Vec<String> = set.into_iter().collect();
    // Defensive: only keep ids that are still installed, then append the new one.
    ids.retain(|x| installed_ids.iter().any(|i| i == x));
    ids.push(id.to_string());
    if let Err(e) = save(&ids) {
        tracing::warn!("merula: failed to auto-activate pack `{id}`: {e}");
    }
}
