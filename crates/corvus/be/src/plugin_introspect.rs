//! Plugin-Manager introspection over the OOP boundary.
//!
//! After the Phase-2 flip the **shell stops loading product plugins** — its
//! `AppState.plugin_host` is empty for the Corvus product, so the platform
//! handlers in `src-tauri/src/ipc/platform/plugin.rs` (`list_plugin_info`,
//! `plugin_dep_graph`, contributions, …) would report nothing. The live host
//! for Corvus plugins now lives **here**, in `corvus-be` (`main.rs` owns
//! `plugin_host: Arc<Mutex<PluginHost>>`). This module re-serves the **read /
//! reflection** subset of that surface as `corvus`-program RPC handlers, so the
//! Plugin Manager (which runs inside the Corvus window, where corvus-be is
//! always up) reads its plugin state from the process that actually owns it.
//!
//! Scope is deliberately the **introspection** subset the Plugin Manager UI
//! reads: discovery, per-plugin info, dep graph, enable/disable previews,
//! dependents, contributions/containers, and the file-backed settings get/set.
//! The runtime **mutations** (enable/disable/reload/delete/uninstall, exec_hook,
//! fire_command, schedulers) are a separate, larger surface — they fire hooks,
//! cancel jobs and emit `arbor://plugins-reloaded`, and several touch shell-side
//! state (jobs registry, per-repo `.arbor/plugins/` across open tabs). They are
//! out of scope for this read-redirection and tracked separately; until they are
//! ported, the Plugin Manager's write actions still route to the (now empty)
//! shell host and will no-op for Corvus plugins. See the FLAG in the agent
//! report.
//!
//! ## Why a module-static host handle
//!
//! Every `#[arbor_rpc::handler]` is handed `&CorvusState`, which deliberately
//! holds only transport-ready pieces (event sink, repo registry, hook broker,
//! reverse channel) — **not** the `PluginHost` (that is mlua-coupled and owned by
//! `main`). Rather than thread the host into `CorvusState` (which would drag the
//! whole plugin-core dependency into the `corvus-core` crate), `main` publishes
//! its `Arc<Mutex<PluginHost>>` here once at boot via [`install`]; the handlers
//! read it through [`host`]. This keeps `CorvusState` host-free and the wiring a
//! single line in `main.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use arbor_plugin_core::prelude::{
    ContainerDef, ContributionPoint, EnablePreview, PluginContribution, PluginHost, PluginInfo,
    global_settings_path, load_settings_file, save_settings_file,
};
use corvus_core::prelude::CorvusState;
use serde::Serialize;

/// The process-wide handle to the plugin host that `main` builds. Set once at
/// boot via [`install`]; read by every handler below. `OnceLock` so it is wired
/// before the serve loop starts and never re-pointed.
static HOST: OnceLock<Arc<Mutex<PluginHost>>> = OnceLock::new();

/// Publish the plugin host so the introspection handlers can read it. Called
/// once from `main` right after the `Arc<Mutex<PluginHost>>` is constructed (and
/// before `serve_stdio`). Idempotent — a second call is ignored.
pub fn install(host: Arc<Mutex<PluginHost>>) {
    let _ = HOST.set(host);
}

/// Borrow the shared plugin-host handle for the **write** handlers
/// (`plugin_lifecycle`, `plugin_reload`, `plugin_scheduler`, `plugin_dispatch`).
/// Same static the read handlers lock via [`with_host`] — one host, one source of
/// truth. Panics only if called before [`install`] (a boot-ordering bug).
pub(crate) fn host() -> Arc<Mutex<PluginHost>> {
    Arc::clone(HOST.get().expect("plugin host not installed in corvus-be"))
}

/// Lock the host, mapping a poisoned/absent lock onto the same error-string shape
/// the shell used (`lock_plugin_host` → `AppError`). Absence means `main` never
/// called [`install`] — a wiring bug, surfaced verbatim.
fn with_host<R>(
    f: impl FnOnce(&PluginHost) -> Result<R, String>,
) -> Result<R, String> {
    let host = HOST
        .get()
        .ok_or_else(|| "plugin host not installed in corvus-be".to_string())?;
    let guard = host
        .lock()
        .map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&guard)
}

// ---------------------------------------------------------------------------
// Per-plugin info + cascade previews — read the live host.
//
// NB: plugin *discovery* (`list_plugins`), the plugins-dir path, the
// installed-plugin-path resolver, and the `plugins_enabled` kill-switch read are
// deliberately NOT served here — they are host-free (disk walks / a shell
// `AppConfig` field) and product-agnostic, so they stay on the `platform`
// program. Only host-backed reflection moves to corvus-be.
// ---------------------------------------------------------------------------

/// Full Plugin-Manager summary of every plugin (loaded, dormant, failed).
#[arbor_rpc::handler]
fn list_plugin_info(_ctx: &CorvusState) -> Result<Vec<PluginInfo>, String> {
    with_host(|h| Ok(h.list_plugin_info()))
}

/// Preview the enable cascade for `name` (plan + blockers). Mirrors the shell's
/// `plugin_enable_preview`.
#[arbor_rpc::handler]
fn plugin_enable_preview(_ctx: &CorvusState, name: String) -> Result<EnablePreview, String> {
    with_host(|h| {
        Ok(EnablePreview {
            plan: h.compute_enable_cascade(&name),
            blockers: h.compute_enable_blockers(&name),
        })
    })
}

/// Preview the disable cascade for `name` (leaves-first, target last).
#[arbor_rpc::handler]
fn plugin_disable_preview(_ctx: &CorvusState, name: String) -> Result<Vec<String>, String> {
    with_host(|h| Ok(h.compute_disable_cascade(&name)))
}

/// Currently-enabled plugins that directly (non-optionally) depend on `name`.
/// Mirrors the shell's `plugin_dependents`.
#[arbor_rpc::handler]
fn plugin_dependents(_ctx: &CorvusState, name: String) -> Result<Vec<String>, String> {
    with_host(|h| {
        let mut out = Vec::new();
        for p in &h.plugins {
            if !p.is_enabled() {
                continue;
            }
            if p.manifest.name == name {
                continue;
            }
            if p
                .manifest
                .dependencies
                .iter()
                .any(|d| d.name == name && !d.optional)
            {
                out.push(p.manifest.name.clone());
            }
        }
        out.sort();
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// Dependency graph — byte-faithful port of the shell's `plugin_dep_graph`.
// The wire shape (`DepGraphNode`/`DepGraphEdge`) matches `src/lib/ipc/plugin.ts`,
// so the FE `PluginDepGraphModal` renders identically whether it reads from the
// shell (pre-flip) or corvus-be (post-flip).
// ---------------------------------------------------------------------------

/// A single node in the dependency graph returned to the frontend.
#[derive(Serialize)]
pub struct DepGraphNode {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub depends_on: Vec<DepGraphEdge>,
    pub dependents: Vec<DepGraphEdge>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct DepGraphEdge {
    pub name: String,
    pub version: String,
    pub optional: bool,
    pub unmet: bool,
}

#[arbor_rpc::handler]
fn plugin_dep_graph(_ctx: &CorvusState) -> Result<Vec<DepGraphNode>, String> {
    with_host(|host| {
        // name -> (version, enabled, declared deps as (name, version, optional), error)
        type DepTriple = (String, String, bool);
        let mut entries: HashMap<String, (String, bool, Vec<DepTriple>, Option<String>)> =
            HashMap::new();
        for p in &host.plugins {
            let deps = p
                .manifest
                .dependencies
                .iter()
                .map(|d| (d.name.clone(), d.version.clone(), d.optional))
                .collect();
            entries.insert(
                p.manifest.name.clone(),
                (p.manifest.version.clone(), p.is_enabled(), deps, None),
            );
        }
        for d in &host.dormant {
            let deps = d
                .manifest
                .dependencies
                .iter()
                .map(|dep| (dep.name.clone(), dep.version.clone(), dep.optional))
                .collect();
            entries.entry(d.manifest.name.clone()).or_insert((
                d.manifest.version.clone(),
                false,
                deps,
                None,
            ));
        }
        for f in &host.load_failures {
            entries.entry(f.name.clone()).or_insert((
                f.version.clone(),
                false,
                Vec::new(),
                Some(f.error.clone()),
            ));
        }

        // Whether dep `(dep_name, version_req)` is unmet against the loaded set.
        let is_unmet = |dep_name: &str, version_req: &str, optional: bool| -> bool {
            match entries.get(dep_name) {
                None => !optional, // missing + not optional → unmet
                Some((v, _, _, _)) => {
                    if version_req.is_empty() {
                        false
                    } else {
                        let ok = semver::VersionReq::parse(version_req)
                            .ok()
                            .zip(semver::Version::parse(v).ok())
                            .map(|(req, vv)| req.matches(&vv))
                            .unwrap_or(true);
                        !ok
                    }
                }
            }
        };

        // Pre-compute dependents.
        let mut dependents: HashMap<String, Vec<DepGraphEdge>> = HashMap::new();
        for (name, (_, _, deps, _)) in &entries {
            for (dn, dv, dopt) in deps {
                let unmet = is_unmet(dn, dv, *dopt);
                dependents.entry(dn.clone()).or_default().push(DepGraphEdge {
                    name: name.clone(),
                    version: entries.get(name).map(|(v, _, _, _)| v.clone()).unwrap_or_default(),
                    optional: *dopt,
                    unmet,
                });
            }
        }

        let mut out: Vec<DepGraphNode> = entries
            .iter()
            .map(|(name, (version, enabled, deps, error))| {
                let depends_on: Vec<DepGraphEdge> = deps
                    .iter()
                    .map(|(dn, dv, dopt)| DepGraphEdge {
                        name: dn.clone(),
                        version: dv.clone(),
                        optional: *dopt,
                        unmet: is_unmet(dn, dv, *dopt),
                    })
                    .collect();
                DepGraphNode {
                    name: name.clone(),
                    version: version.clone(),
                    enabled: *enabled,
                    depends_on,
                    dependents: dependents.get(name).cloned().unwrap_or_default(),
                    error: error.clone(),
                }
            })
            .collect();

        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// Contributions + containers — pure reflection off the live host's registry.
// ---------------------------------------------------------------------------

/// All contributions, optionally filtered by point. Mirrors the shell's
/// `list_plugin_contributions`.
#[arbor_rpc::handler]
fn list_plugin_contributions(
    _ctx: &CorvusState,
    point: Option<String>,
) -> Result<Vec<PluginContribution>, String> {
    with_host(|h| {
        Ok(match point {
            Some(p) => h.contributions.list_for_point(&p),
            None => h.contributions.list_all(),
        })
    })
}

/// Declared contribution points (informational). Mirrors `list_contribution_points`.
#[arbor_rpc::handler]
fn list_contribution_points(_ctx: &CorvusState) -> Result<Vec<ContributionPoint>, String> {
    with_host(|h| Ok(h.contributions.list_points()))
}

/// All registered containers. Mirrors the shell's `list_containers`.
#[arbor_rpc::handler]
fn list_containers(_ctx: &CorvusState) -> Result<Vec<ContainerDef>, String> {
    with_host(|h| Ok(h.contributions.list_containers()))
}

/// Single container by `<plugin>::<id>` key, or `None`. Mirrors `get_container`.
#[arbor_rpc::handler]
fn get_container(_ctx: &CorvusState, key: String) -> Result<Option<ContainerDef>, String> {
    with_host(|h| Ok(h.contributions.get_container(&key)))
}

// ---------------------------------------------------------------------------
// Plugin settings — plain file-backed (no host); identical to the shell's
// `plugin_settings_get` / `plugin_settings_set_all`. The settings live in the
// per-plugin `global.json`, resolved by `global_settings_path` (shared across
// processes — same path the shell wrote), so reads/writes stay coherent.
// ---------------------------------------------------------------------------

/// All stored settings for a plugin as a JSON object.
#[arbor_rpc::handler]
fn plugin_settings_get(
    _ctx: &CorvusState,
    name: String,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    Ok(load_settings_file(&global_settings_path(&name)))
}

/// Overwrite all settings for a plugin with the provided JSON object.
#[arbor_rpc::handler]
fn plugin_settings_set_all(
    _ctx: &CorvusState,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    save_settings_file(&global_settings_path(&name), &values);
    Ok(())
}
