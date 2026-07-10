//! Build the sync **bundle** from local on-disk state, and fingerprint it.
//!
//! Every source is derived from the shell-pushed corvus product dir
//! (`corvus_config_dir` = `…/profiles/<p>/corvus`): the profile root is its
//! parent, and `plugins/` is a sibling. Machine-specific and heavy data are
//! filtered out here (absolute repo paths dropped, UI settings cherry-picked,
//! the `[sync]` table stripped from the corvus config, plugin data size-capped).

use std::path::{Path, PathBuf};

use corvus_core::prelude::CorvusState;

use crate::corvus_config::SyncConfig;

use super::BundleFile;

/// Top-level `profile.toml` keys that are safe to sync (UI/appearance only —
/// NOT `ide`/`terminals`/`git` which hold machine-specific absolute paths).
const PROFILE_KEYS: &[&str] = &["theme", "appearance", "animations", "keybindings", "activity_bar"];

/// The three roots the sync sources live under, all derived from the shell-
/// pushed corvus dir (its parent is the profile root; `plugins/` is a sibling).
pub(crate) struct Dirs {
    pub corvus: PathBuf,
    pub profile: PathBuf,
    pub plugins: PathBuf,
}

/// Resolve [`Dirs`]. Errors only when the shell hasn't pushed the corvus dir yet.
pub(crate) fn resolve_dirs(state: &CorvusState) -> Result<Dirs, String> {
    let corvus = PathBuf::from(crate::corvus_config::corvus_config_dir(state)?);
    let profile = corvus
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "sync: cannot resolve profile dir from corvus dir".to_string())?;
    let plugins = profile.join("plugins");
    Ok(Dirs { corvus, profile, plugins })
}

/// Build the full bundle for the given include-toggles. Returns an error only if
/// the corvus dir isn't resolvable yet (shell hasn't pushed it) — individual
/// missing source files are simply skipped.
pub(crate) fn build(state: &CorvusState, cfg: &SyncConfig) -> Result<Vec<BundleFile>, String> {
    let Dirs { corvus: corvus_dir, profile: profile_dir, plugins: plugins_dir } =
        resolve_dirs(state)?;

    let mut files: Vec<BundleFile> = Vec::new();

    // Manifest — carries a timestamp, so it's excluded from the fingerprint.
    let manifest = serde_json::json!({
        "schema_version": super::SCHEMA_VERSION,
        "product":        "corvus",
        "machine":        super::machine_id(),
        "pushed_at":      super::now_epoch(),
    });
    files.push(BundleFile {
        path:  super::F_MANIFEST.to_string(),
        bytes: serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    });

    if cfg.include_workspaces {
        if let Some(bytes) = read_opt(&corvus_dir.join("workspaces.json")) {
            files.push(BundleFile { path: super::F_WORKSPACES.to_string(), bytes });
        }
        if let Some(map) = repos_identity(&corvus_dir.join("repos.json")) {
            files.push(BundleFile {
                path:  super::F_REPOS.to_string(),
                bytes: serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?,
            });
        }
    }

    if cfg.include_settings {
        if let Some(text) = profile_subset(&profile_dir.join("profile.toml")) {
            files.push(BundleFile { path: super::F_SETTINGS_PROFILE.to_string(), bytes: text.into_bytes() });
        }
        if let Some(text) = corvus_settings(&corvus_dir.join("config.toml")) {
            files.push(BundleFile { path: super::F_SETTINGS_CORVUS.to_string(), bytes: text.into_bytes() });
        }
    }

    if cfg.include_mods {
        let list = mod_list(&plugins_dir);
        files.push(BundleFile {
            path:  super::F_MODS.to_string(),
            bytes: serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?,
        });
    }

    if cfg.include_plugin_data {
        let cap_bytes = cfg.plugin_data_cap_kb.saturating_mul(1024);
        files.extend(plugin_data(&plugins_dir, cap_bytes));
    }

    Ok(files)
}

/// A deterministic fingerprint over the bundle's content (manifest excluded, so
/// the per-push timestamp doesn't make every build look changed). `DefaultHasher`
/// uses fixed keys, so it's stable across process runs — good enough to detect
/// "did anything actually change since the last push".
pub(crate) fn fingerprint(files: &[BundleFile]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut items: Vec<&BundleFile> = files.iter().filter(|f| f.path != super::F_MANIFEST).collect();
    items.sort_by(|a, b| a.path.cmp(&b.path));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in items {
        f.path.hash(&mut h);
        f.bytes.hash(&mut h);
    }
    h.finish()
}

/// The fixed (non-plugin-data) bundle paths — used by pull to fetch the known
/// files; plugin-data paths are discovered from the fetched mod list.
pub(crate) fn fixed_paths() -> Vec<String> {
    vec![
        super::F_MANIFEST.to_string(),
        super::F_WORKSPACES.to_string(),
        super::F_REPOS.to_string(),
        super::F_SETTINGS_PROFILE.to_string(),
        super::F_SETTINGS_CORVUS.to_string(),
        super::F_MODS.to_string(),
    ]
}

// ── source readers / transforms ───────────────────────────────────────────────

fn read_opt(p: &Path) -> Option<Vec<u8>> {
    std::fs::read(p).ok()
}

/// `repos.json` → `{ <id>: { remote_url, display_name } }`, keeping only repos
/// that carry a `remote_url` (the sole cross-machine identity). Absolute paths
/// are intentionally dropped.
fn repos_identity(p: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(p).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let entries = value.get("entries")?.as_array()?;
    let mut map = serde_json::Map::new();
    for e in entries {
        let Some(id) = e.get("id").and_then(|x| x.as_str()) else { continue };
        let Some(remote) = e.get("remote_url").and_then(|x| x.as_str()) else { continue };
        let name = e.get("display_name").and_then(|x| x.as_str()).unwrap_or("");
        map.insert(
            id.to_string(),
            serde_json::json!({ "remote_url": remote, "display_name": name }),
        );
    }
    Some(serde_json::Value::Object(map))
}

/// `profile.toml` cherry-picked to the UI-only keys ([`PROFILE_KEYS`]).
fn profile_subset(p: &Path) -> Option<String> {
    let text = std::fs::read_to_string(p).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let table = value.as_table()?;
    let mut out = toml::value::Table::new();
    for k in PROFILE_KEYS {
        if let Some(v) = table.get(*k) {
            out.insert((*k).to_string(), v.clone());
        }
    }
    if out.is_empty() {
        return None;
    }
    toml::to_string_pretty(&toml::Value::Table(out)).ok()
}

/// The corvus `config.toml` minus the `[sync]` table (which holds this machine's
/// status + repo target — must not travel, and must not perturb the fingerprint).
fn corvus_settings(p: &Path) -> Option<String> {
    let text = std::fs::read_to_string(p).ok()?;
    let mut value: toml::Value = toml::from_str(&text).ok()?;
    if let Some(table) = value.as_table_mut() {
        table.remove("sync");
    }
    toml::to_string_pretty(&value).ok()
}

/// The installed-mod list `{ plugins: [ { name, version, enabled } ] }`, merged
/// from the marketplace ledger + the enable-state file.
fn mod_list(plugins_dir: &Path) -> serde_json::Value {
    let installed = std::fs::read_to_string(plugins_dir.join("marketplace_installed.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let states = std::fs::read_to_string(plugins_dir.join("plugin_states.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let states_map = states.as_ref().and_then(|v| v.as_object());

    let mut arr = Vec::new();
    if let Some(plugins) = installed
        .as_ref()
        .and_then(|v| v.get("plugins"))
        .and_then(|v| v.as_object())
    {
        for (name, meta) in plugins {
            let version = meta.get("version").and_then(|x| x.as_str()).unwrap_or("");
            let enabled = states_map
                .and_then(|m| m.get(name))
                .and_then(|x| x.as_bool())
                .or_else(|| meta.get("enabled").and_then(|x| x.as_bool()))
                .unwrap_or(true);
            arr.push(serde_json::json!({ "name": name, "version": version, "enabled": enabled }));
        }
    }
    arr.sort_by(|a, b| {
        a.get("name").and_then(|x| x.as_str()).unwrap_or("")
            .cmp(b.get("name").and_then(|x| x.as_str()).unwrap_or(""))
    });
    serde_json::json!({ "plugins": arr })
}

/// Each plugin's small `global.json`, skipping any over the byte cap.
fn plugin_data(plugins_dir: &Path, cap_bytes: u64) -> Vec<BundleFile> {
    let data_dir = plugins_dir.join("plugin_data");
    let mut out = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(&data_dir) else { return out };
    for entry in read_dir.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let global = entry.path().join("global.json");
        let Ok(meta) = std::fs::metadata(&global) else { continue };
        if !meta.is_file() {
            continue;
        }
        if cap_bytes > 0 && meta.len() > cap_bytes {
            continue; // heavy blob — keep it out of the settings bundle
        }
        if let Ok(bytes) = std::fs::read(&global) {
            out.push(BundleFile {
                path: format!("{}{}/global.json", super::PLUGIN_DATA_PREFIX, name),
                bytes,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}
