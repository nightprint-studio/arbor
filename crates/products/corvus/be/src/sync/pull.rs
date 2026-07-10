//! Pull: read the remote bundle, diff it against local state into a [`PullPlan`],
//! and apply the user's per-item selections ([`PullSelections`]).
//!
//! The crux is **repo identity**: the remote `repos.json` keys workspaces' repo
//! ids to `remote_url`s, never paths. On apply we map each remote repo id to a
//! *local* registry id (matched by `remote_url`, or a fresh pending entry when
//! the repo isn't cloned here yet), then rewrite the imported workspaces'
//! `repo_ids`. Unmatched remotes surface as [`MissingRepo`]s the UI can offer to
//! clone/locate — the existing missing-project flow handles empty-path entries.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::corvus_config::{self, CorvusConfig, SyncConfig};
use crate::workspace::registry as ws_registry;
use crate::workspace::store::{self as ws_store, WorkspaceStore};

use super::{engine, remote, sources};

// ── wire contract ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(crate) struct PullPlan {
    pub available: bool,
    pub workspaces: Vec<WsItem>,
    pub settings: Vec<SettingsItem>,
    pub mods: Vec<ModItem>,
    pub plugin_data: Vec<DataItem>,
    pub missing_repos: Vec<MissingRepo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WsItem {
    pub id: String,
    pub name: String,
    /// `new` (absent locally), `changed` (differs), or `same`.
    pub status: String,
    pub repo_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct SettingsItem {
    pub key: String, // "profile" | "corvus"
    pub label: String,
    pub differs: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModItem {
    pub name: String,
    pub version: String,
    pub installed: bool,
    /// Remote enable state (applied to installed mods when `mod_enable` is set).
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DataItem {
    pub name: String,
    pub differs: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MissingRepo {
    pub remote_url: String,
    pub display_name: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PullSelections {
    #[serde(default)] pub workspace_ids: Vec<String>,
    #[serde(default)] pub settings_keys: Vec<String>, // "profile" | "corvus"
    #[serde(default)] pub mod_enable: bool,
    #[serde(default)] pub plugin_data_names: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct PullSummary {
    pub workspaces_applied: usize,
    pub settings_applied: usize,
    pub mods_enabled: usize,
    pub plugin_data_applied: usize,
    pub missing_repos: Vec<MissingRepo>,
    /// The UI/profile settings changed — the FE should reload its config stores.
    pub settings_reload_needed: bool,
}

// ── preview ────────────────────────────────────────────────────────────────────

pub(crate) async fn preview(state: &CorvusState) -> Result<PullPlan, String> {
    let cfg = corvus_config::load(state).sync;
    let (_target, remote_map) = fetch_all(&cfg).await?;
    let available = remote_map.keys().any(|k| k != super::F_MANIFEST);
    if !available {
        return Ok(PullPlan {
            available: false,
            workspaces: Vec::new(),
            settings: Vec::new(),
            mods: Vec::new(),
            plugin_data: Vec::new(),
            missing_repos: Vec::new(),
        });
    }
    let local = local_map(state);

    // Workspaces — status vs the local store.
    let local_ws = parse_ws(local.get(super::F_WORKSPACES));
    let remote_ws = parse_ws(remote_map.get(super::F_WORKSPACES));
    let mut workspaces = Vec::new();
    for w in &remote_ws.workspaces {
        if w.id == crate::workspace::SCRATCH_ID {
            continue;
        }
        let status = match local_ws.workspaces.iter().find(|l| l.id == w.id) {
            None => "new",
            Some(l) if l.name != w.name || l.repo_ids != w.repo_ids => "changed",
            Some(_) => "same",
        };
        workspaces.push(WsItem {
            id: w.id.clone(),
            name: w.name.clone(),
            status: status.to_string(),
            repo_count: w.repo_ids.len(),
        });
    }

    // Settings.
    let mut settings = Vec::new();
    for (key, path, label) in [
        ("profile", super::F_SETTINGS_PROFILE, "UI settings"),
        ("corvus", super::F_SETTINGS_CORVUS, "Git preferences"),
    ] {
        if let Some(remote_bytes) = remote_map.get(path) {
            let differs = local.get(path).map(|l| l != remote_bytes).unwrap_or(true);
            settings.push(SettingsItem { key: key.to_string(), label: label.to_string(), differs });
        }
    }

    // Mods.
    let local_mod_names: HashSet<String> =
        parse_mods(local.get(super::F_MODS)).into_iter().map(|m| m.0).collect();
    let mods = parse_mods(remote_map.get(super::F_MODS))
        .into_iter()
        .map(|(name, version, enabled)| ModItem {
            installed: local_mod_names.contains(&name),
            name,
            version,
            enabled,
        })
        .collect();

    // Plugin data.
    let mut plugin_data = Vec::new();
    for (path, remote_bytes) in &remote_map {
        if let Some(name) = plugin_data_name(path) {
            let differs = local.get(path).map(|l| l != remote_bytes).unwrap_or(true);
            plugin_data.push(DataItem { name, differs });
        }
    }
    plugin_data.sort_by(|a, b| a.name.cmp(&b.name));

    let missing_repos = missing_repos(state, remote_map.get(super::F_REPOS));

    Ok(PullPlan { available: true, workspaces, settings, mods, plugin_data, missing_repos })
}

// ── apply ──────────────────────────────────────────────────────────────────────

pub(crate) async fn apply(state: &CorvusState, sel: PullSelections) -> Result<PullSummary, String> {
    let cfg = corvus_config::load(state).sync;
    let (_target, remote_map) = fetch_all(&cfg).await?;
    let dirs = sources::resolve_dirs(state)?;
    let mut summary = PullSummary::default();

    // ── workspaces (with repo-id remap by remote_url) ──
    if !sel.workspace_ids.is_empty() {
        let remote_ws = parse_ws(remote_map.get(super::F_WORKSPACES));
        let remote_repos = parse_repos_map(remote_map.get(super::F_REPOS));

        // Ensure a local registry entry exists for every referenced remote repo,
        // building remote_id → local_id.
        let mut id_map: HashMap<String, String> = HashMap::new();
        ws_registry::mutate(state, |reg| {
            for (rid, (url, name)) in &remote_repos {
                let local_id = match reg.find_by_remote_url(url) {
                    Some(e) => e.id.clone(),
                    None => reg.insert_pending(Some(url.clone()), name),
                };
                id_map.insert(rid.clone(), local_id);
            }
            Ok(())
        })?;

        let selected: HashSet<&String> = sel.workspace_ids.iter().collect();
        summary.workspaces_applied = ws_store::mutate(state, |store| {
            let mut n = 0;
            for w in &remote_ws.workspaces {
                if w.id == crate::workspace::SCRATCH_ID || !selected.contains(&w.id) {
                    continue;
                }
                let mut def = w.clone();
                def.repo_ids = w.repo_ids.iter().filter_map(|r| id_map.get(r).cloned()).collect();
                match store.get_mut(&w.id) {
                    Some(existing) => *existing = def,
                    None => store.workspaces.push(def),
                }
                n += 1;
            }
            Ok(n)
        })?;
    }

    // ── settings ──
    if sel.settings_keys.iter().any(|k| k == "corvus") {
        if let Some(bytes) = remote_map.get(super::F_SETTINGS_CORVUS) {
            apply_corvus_settings(state, &String::from_utf8_lossy(bytes))?;
            summary.settings_applied += 1;
        }
    }
    if sel.settings_keys.iter().any(|k| k == "profile") {
        if let Some(bytes) = remote_map.get(super::F_SETTINGS_PROFILE) {
            apply_profile_subset(&dirs.profile.join("profile.toml"), &String::from_utf8_lossy(bytes))?;
            summary.settings_applied += 1;
            summary.settings_reload_needed = true;
        }
    }

    // ── mod enable states ──
    if sel.mod_enable {
        if let Some(bytes) = remote_map.get(super::F_MODS) {
            summary.mods_enabled = apply_mod_enable(&dirs.plugins, bytes)?;
        }
    }

    // ── plugin data ──
    for name in &sel.plugin_data_names {
        let path = format!("{}{}/global.json", super::PLUGIN_DATA_PREFIX, name);
        if let Some(bytes) = remote_map.get(&path) {
            write_plugin_data(&dirs.plugins, name, bytes)?;
            summary.plugin_data_applied += 1;
        }
    }

    summary.missing_repos = missing_repos(state, remote_map.get(super::F_REPOS));

    let _ = corvus_config::update_sync(state, |s| s.last_pull_at = Some(super::now_epoch()));
    crate::workspace::emit_registry_changed(state);
    state.emit(
        "arbor://corvus-sync-pulled",
        serde_json::json!({ "at": super::now_epoch(), "reload_settings": summary.settings_reload_needed }),
    );
    Ok(summary)
}

// ── remote fetch + local build ──────────────────────────────────────────────────

/// Fetch the whole remote bundle (fixed files + per-plugin data discovered from
/// the mod list) into `path → bytes`.
async fn fetch_all(cfg: &SyncConfig) -> Result<(remote::SyncRemote, HashMap<String, Vec<u8>>), String> {
    let target = remote::from_config(cfg).ok_or_else(|| "Sync is not configured.".to_string())?;
    let fixed = engine::fetch(&target, &sources::fixed_paths()).await?;
    let mut data_paths = Vec::new();
    if let Some(list) = fixed.iter().find(|f| f.path == super::F_MODS) {
        for (name, _, _) in parse_mods(Some(&list.bytes)) {
            data_paths.push(format!("{}{}/global.json", super::PLUGIN_DATA_PREFIX, name));
        }
    }
    let data = engine::fetch(&target, &data_paths).await?;
    let map = fixed.into_iter().chain(data).map(|f| (f.path, f.bytes)).collect();
    Ok((target, map))
}

/// The locally-built bundle (all includes forced on) as `path → bytes`, for diffing.
fn local_map(state: &CorvusState) -> HashMap<String, Vec<u8>> {
    let mut cfg = corvus_config::load(state).sync;
    cfg.include_workspaces = true;
    cfg.include_settings = true;
    cfg.include_mods = true;
    cfg.include_plugin_data = true;
    sources::build(state, &cfg)
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.path, f.bytes))
        .collect()
}

// ── parsers ──────────────────────────────────────────────────────────────────────

fn parse_ws(bytes: Option<&Vec<u8>>) -> WorkspaceStore {
    bytes.and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default()
}

fn parse_mods(bytes: Option<&Vec<u8>>) -> Vec<(String, String, bool)> {
    let Some(bytes) = bytes else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<Value>(bytes) else { return Vec::new() };
    let Some(arr) = v.get("plugins").and_then(|p| p.as_array()) else { return Vec::new() };
    arr.iter()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let version = m.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let enabled = m.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
            Some((name, version, enabled))
        })
        .collect()
}

/// `repos.json` → `remote_id → (remote_url, display_name)` (entries without a
/// url are skipped — they can't be matched across machines).
fn parse_repos_map(bytes: Option<&Vec<u8>>) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    let Some(bytes) = bytes else { return out };
    let Ok(v) = serde_json::from_slice::<Value>(bytes) else { return out };
    if let Some(obj) = v.as_object() {
        for (id, meta) in obj {
            let url = meta.get("remote_url").and_then(|x| x.as_str()).unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let name = meta.get("display_name").and_then(|x| x.as_str()).unwrap_or("");
            out.insert(id.clone(), (url.to_string(), name.to_string()));
        }
    }
    out
}

fn plugin_data_name(path: &str) -> Option<String> {
    let rest = path.strip_prefix(super::PLUGIN_DATA_PREFIX)?;
    let name = rest.strip_suffix("/global.json")?;
    if name.is_empty() || name.contains('/') {
        return None;
    }
    Some(name.to_string())
}

fn missing_repos(state: &CorvusState, bytes: Option<&Vec<u8>>) -> Vec<MissingRepo> {
    let repos = parse_repos_map(bytes);
    let reg = ws_registry::registry(state);
    let mut out: Vec<MissingRepo> = repos
        .into_values()
        .filter(|(url, _)| reg.find_by_remote_url(url).is_none())
        .map(|(url, name)| MissingRepo { remote_url: url, display_name: name })
        .collect();
    out.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    out
}

// ── writers ──────────────────────────────────────────────────────────────────────

/// Overwrite the local corvus git config with the remote one, preserving the
/// local `[sync]` section (machine-specific).
fn apply_corvus_settings(state: &CorvusState, remote_text: &str) -> Result<(), String> {
    let remote_cfg: CorvusConfig =
        toml::from_str(remote_text).map_err(|e| format!("sync: parse remote corvus config: {e}"))?;
    let keep_sync = corvus_config::load(state).sync;
    let mut merged = remote_cfg;
    merged.sync = keep_sync;
    corvus_config::save(state, &merged)
}

/// Overlay the remote UI-settings subset onto the local `profile.toml` (the shell
/// owns it; the FE reloads config on the `arbor://corvus-sync-pulled` event).
fn apply_profile_subset(path: &Path, remote_text: &str) -> Result<(), String> {
    let remote_val: toml::Value =
        toml::from_str(remote_text).map_err(|e| format!("sync: parse remote profile: {e}"))?;
    let remote_tbl = remote_val.as_table().ok_or_else(|| "sync: remote profile not a table".to_string())?;

    let mut local_val: toml::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_else(|| toml::Value::Table(toml::value::Table::new()));
    {
        let local_tbl = local_val
            .as_table_mut()
            .ok_or_else(|| "sync: local profile not a table".to_string())?;
        for (k, v) in remote_tbl {
            local_tbl.insert(k.clone(), v.clone());
        }
    }
    let text = toml::to_string_pretty(&local_val).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Apply remote enable/disable states to the mods that are installed locally.
fn apply_mod_enable(plugins_dir: &Path, remote_mods: &[u8]) -> Result<usize, String> {
    let states_path = plugins_dir.join("plugin_states.json");
    let mut states: serde_json::Map<String, Value> = std::fs::read_to_string(&states_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let mut n = 0;
    for (name, _version, enabled) in parse_mods(Some(&remote_mods.to_vec())) {
        if is_installed(plugins_dir, &name) {
            states.insert(name, Value::Bool(enabled));
            n += 1;
        }
    }
    let text = serde_json::to_string_pretty(&Value::Object(states)).map_err(|e| e.to_string())?;
    std::fs::write(&states_path, text).map_err(|e| e.to_string())?;
    Ok(n)
}

fn is_installed(plugins_dir: &Path, name: &str) -> bool {
    plugins_dir.join("installed").join(name).is_dir()
        || plugins_dir.join("marketplace_plugins").join(name).is_dir()
}

fn write_plugin_data(plugins_dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = plugins_dir.join("plugin_data").join(name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("global.json"), bytes).map_err(|e| e.to_string())
}
