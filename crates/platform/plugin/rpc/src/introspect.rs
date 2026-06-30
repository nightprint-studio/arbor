//! Plugin-Manager **introspection** — read/reflection off the live host. Generic
//! port of `corvus-be`'s former `plugin_introspect.rs`; the wire shapes match
//! `src/lib/ipc/plugin.ts` so the FE renders identically whichever backend serves.

use std::collections::HashMap;

use arbor_plugin_core::prelude::{
    global_settings_path, load_settings_file, save_settings_file, ContainerDef, ContributionPoint,
    EnablePreview, PluginContribution, PluginInfo,
};
use serde::Serialize;

use crate::context::{with_host, PluginRpcContext};

/// Full Plugin-Manager summary of every plugin (loaded, dormant, failed).
pub fn list_plugin_info<C: PluginRpcContext>(ctx: &C) -> Result<Vec<PluginInfo>, String> {
    with_host(ctx, |h| Ok(h.list_plugin_info()))
}

/// Preview the enable cascade for `name` (plan + blockers).
pub fn plugin_enable_preview<C: PluginRpcContext>(
    ctx: &C,
    name: String,
) -> Result<EnablePreview, String> {
    with_host(ctx, |h| {
        Ok(EnablePreview {
            plan: h.compute_enable_cascade(&name),
            blockers: h.compute_enable_blockers(&name),
        })
    })
}

/// Preview the disable cascade for `name` (leaves-first, target last).
pub fn plugin_disable_preview<C: PluginRpcContext>(
    ctx: &C,
    name: String,
) -> Result<Vec<String>, String> {
    with_host(ctx, |h| Ok(h.compute_disable_cascade(&name)))
}

/// Currently-enabled plugins that directly (non-optionally) depend on `name`.
pub fn plugin_dependents<C: PluginRpcContext>(
    ctx: &C,
    name: String,
) -> Result<Vec<String>, String> {
    with_host(ctx, |h| {
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
// Dependency graph — byte-faithful with the shell's `plugin_dep_graph`.
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

pub fn plugin_dep_graph<C: PluginRpcContext>(ctx: &C) -> Result<Vec<DepGraphNode>, String> {
    with_host(ctx, |host| {
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

/// All contributions, optionally filtered by point.
pub fn list_plugin_contributions<C: PluginRpcContext>(
    ctx: &C,
    point: Option<String>,
) -> Result<Vec<PluginContribution>, String> {
    with_host(ctx, |h| {
        Ok(match point {
            Some(p) => h.contributions.list_for_point(&p),
            None => h.contributions.list_all(),
        })
    })
}

/// Declared contribution points (informational).
pub fn list_contribution_points<C: PluginRpcContext>(
    ctx: &C,
) -> Result<Vec<ContributionPoint>, String> {
    with_host(ctx, |h| Ok(h.contributions.list_points()))
}

/// All registered containers.
pub fn list_containers<C: PluginRpcContext>(ctx: &C) -> Result<Vec<ContainerDef>, String> {
    with_host(ctx, |h| Ok(h.contributions.list_containers()))
}

/// Single container by `<plugin>::<id>` key, or `None`.
pub fn get_container<C: PluginRpcContext>(
    ctx: &C,
    key: String,
) -> Result<Option<ContainerDef>, String> {
    with_host(ctx, |h| Ok(h.contributions.get_container(&key)))
}

// ---------------------------------------------------------------------------
// Plugin settings — file-backed (no host); resolved by `global_settings_path`,
// the same per-plugin `global.json` the shell wrote, so reads/writes stay
// coherent across processes. `ctx` is unused (kept for handler uniformity).
// ---------------------------------------------------------------------------

/// All stored settings for a plugin as a JSON object.
pub fn plugin_settings_get<C: PluginRpcContext>(
    _ctx: &C,
    name: String,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    Ok(load_settings_file(&global_settings_path(&name)))
}

/// Overwrite all settings for a plugin with the provided JSON object.
pub fn plugin_settings_set_all<C: PluginRpcContext>(
    _ctx: &C,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    save_settings_file(&global_settings_path(&name), &values);
    Ok(())
}
