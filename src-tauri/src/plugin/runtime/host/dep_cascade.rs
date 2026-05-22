//! Dependency-aware enable/disable cascade.
//!
//! `disable_plugin` and `enable_plugin` originally flipped a single plugin
//! without considering the dependency graph: disabling a plugin left every
//! dependent running with broken `service.call` / hook expectations, and
//! enabling a plugin whose required deps were off silently activated it
//! into a known-broken state.
//!
//! This module computes the cascade for both directions and exposes
//! preview helpers the frontend uses to drive a confirmation modal. Only
//! `required` (non-optional) dependencies participate in the cascade —
//! optional deps stay enabled when their target goes off.

use serde::Serialize;

use super::PluginHost;

// ---------------------------------------------------------------------------
// Preview DTOs
// ---------------------------------------------------------------------------

/// Why a plugin can't be enabled. Surfaced verbatim in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct EnableBlocker {
    /// Name of the missing/broken dependency.
    pub name:        String,
    /// Declared version requirement (empty when the manifest left it loose).
    pub version_req: String,
    /// Human-readable reason: "not installed", "version mismatch: needs X,
    /// have Y", "load failure: <error>".
    pub reason:      String,
}

/// Result of the enable-preview IPC. `plan` lists the names that would be
/// enabled in order (deps first, target last) — including any already-disabled
/// transitive required deps. `blockers` is non-empty when at least one
/// required dep is missing or unloadable; in that case `plan` is best-effort
/// (it lists what *would* be enabled if the blockers were resolved) but the
/// host refuses to actually run the cascade.
#[derive(Debug, Clone, Serialize)]
pub struct EnablePreview {
    pub plan:     Vec<String>,
    pub blockers: Vec<EnableBlocker>,
}

// ---------------------------------------------------------------------------
// PluginHost extensions
// ---------------------------------------------------------------------------

impl PluginHost {
    /// Names of plugins currently enabled that declare a required dependency
    /// on `name`. Direct dependents only — the cascade walker chases the rest.
    fn direct_required_dependents(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for p in &self.plugins {
            if !p.is_enabled() { continue; }
            if p.manifest.name == name { continue; }
            if p.manifest.dependencies.iter()
                .any(|d| d.name == name && !d.optional)
            {
                out.push(p.manifest.name.clone());
            }
        }
        out
    }

    /// Full disable cascade for `name`: every enabled plugin that (transitively)
    /// requires it, ordered leaves-first. `name` itself is the LAST entry so
    /// the caller can iterate the vec and call `disable_one_plugin` in order
    /// without orphaning anything.
    ///
    /// Returns an empty vec when `name` is not currently enabled.
    pub fn compute_disable_cascade(&self, name: &str) -> Vec<String> {
        // Only enabled plugins participate — dormant / failed ones can't
        // become broken because they were never running.
        let enabled = self.plugins.iter().any(|p| p.manifest.name == name && p.is_enabled());
        if !enabled { return Vec::new(); }

        // DFS post-order from `name` over the reverse-edge graph (dependent →
        // dependee). Post-order naturally produces leaves-first.
        let mut visited = std::collections::HashSet::<String>::new();
        let mut order   = Vec::<String>::new();

        fn visit(
            host:    &PluginHost,
            node:    &str,
            visited: &mut std::collections::HashSet<String>,
            order:   &mut Vec<String>,
        ) {
            if !visited.insert(node.to_string()) { return; }
            for dep in host.direct_required_dependents(node) {
                visit(host, &dep, visited, order);
            }
            order.push(node.to_string());
        }

        visit(self, name, &mut visited, &mut order);
        order
    }

    /// Required dependencies of `name` that are currently NOT enabled but
    /// exist on disk (dormant or already enabled). Used as the building block
    /// for the enable cascade.
    fn required_disabled_deps(&self, name: &str) -> Vec<String> {
        let manifest = self.manifest_for(name);
        let Some(manifest) = manifest else { return Vec::new(); };
        let mut out = Vec::new();
        for d in &manifest.dependencies {
            if d.optional { continue; }
            // Already enabled? skip.
            if self.plugins.iter().any(|p| p.manifest.name == d.name && p.is_enabled()) {
                continue;
            }
            // Present (dormant or disabled-but-loaded)? include.
            let known = self.dormant.iter().any(|x| x.manifest.name == d.name)
                || self.plugins.iter().any(|p| p.manifest.name == d.name);
            if known { out.push(d.name.clone()); }
        }
        out
    }

    /// Look up the manifest for `name` whether the plugin is live, dormant
    /// or load-failed (failed entries don't keep their manifest, so they're
    /// skipped — callers treat that as "blocker" via `compute_enable_blockers`).
    fn manifest_for(&self, name: &str)
        -> Option<&crate::plugin::runtime::PluginManifest>
    {
        if let Some(p) = self.plugins.iter().find(|p| p.manifest.name == name) {
            return Some(&p.manifest);
        }
        if let Some(d) = self.dormant.iter().find(|d| d.manifest.name == name) {
            return Some(&d.manifest);
        }
        None
    }

    /// Full enable cascade for `name`: required deps not currently enabled,
    /// topologically ordered (deps before dependents), with `name` last.
    /// Already-enabled plugins are skipped. Use `compute_enable_blockers`
    /// alongside to detect plans that can't actually run.
    pub fn compute_enable_cascade(&self, name: &str) -> Vec<String> {
        let mut visited = std::collections::HashSet::<String>::new();
        let mut order   = Vec::<String>::new();

        fn visit(
            host:    &PluginHost,
            node:    &str,
            visited: &mut std::collections::HashSet<String>,
            order:   &mut Vec<String>,
        ) {
            if !visited.insert(node.to_string()) { return; }
            for dep in host.required_disabled_deps(node) {
                visit(host, &dep, visited, order);
            }
            // Already enabled? Don't include — caller would no-op anyway and
            // the UI would print misleading "enabling X" lines.
            let already_on = host.plugins.iter()
                .any(|p| p.manifest.name == node && p.is_enabled());
            if !already_on {
                order.push(node.to_string());
            }
        }

        visit(self, name, &mut visited, &mut order);
        order
    }

    /// Required deps of `name` that the host can't enable: missing entirely,
    /// failed to load (cycle / manifest broken), or installed with a version
    /// that doesn't satisfy the manifest's `version` requirement.
    pub fn compute_enable_blockers(&self, name: &str) -> Vec<EnableBlocker> {
        let manifest = self.manifest_for(name);
        let Some(manifest) = manifest else {
            // `name` itself isn't loaded. The IPC layer treats this as
            // "plugin not found" higher up — return empty here.
            return Vec::new();
        };

        let mut out = Vec::new();
        for d in &manifest.dependencies {
            if d.optional { continue; }

            // load_failures don't carry their manifest, so a dep listed there
            // counts as unusable regardless of version.
            if let Some(f) = self.load_failures.iter().find(|f| f.name == d.name) {
                out.push(EnableBlocker {
                    name:        d.name.clone(),
                    version_req: d.version.clone(),
                    reason:      format!("failed to load: {}", f.error),
                });
                continue;
            }

            // Available version: prefer the live plugin if it exists,
            // otherwise check the dormant manifest.
            let installed_ver = self.plugins.iter()
                .find(|p| p.manifest.name == d.name)
                .map(|p| p.manifest.version.clone())
                .or_else(|| self.dormant.iter()
                    .find(|x| x.manifest.name == d.name)
                    .map(|x| x.manifest.version.clone()));

            let Some(have) = installed_ver else {
                out.push(EnableBlocker {
                    name:        d.name.clone(),
                    version_req: d.version.clone(),
                    reason:      "not installed".to_string(),
                });
                continue;
            };

            if !d.version.is_empty() {
                let ok = semver::VersionReq::parse(&d.version)
                    .ok()
                    .zip(semver::Version::parse(&have).ok())
                    .map(|(req, v)| req.matches(&v))
                    .unwrap_or(true); // permissive on malformed semver
                if !ok {
                    out.push(EnableBlocker {
                        name:        d.name.clone(),
                        version_req: d.version.clone(),
                        reason:      format!(
                            "version mismatch: needs {}, have {}",
                            d.version, have
                        ),
                    });
                }
            }
        }

        out
    }
}
