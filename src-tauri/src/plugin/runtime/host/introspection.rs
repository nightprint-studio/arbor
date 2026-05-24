//! `list_plugin_info` — frontend-facing summary of every plugin (loaded,
//! dormant, or failed).

use std::sync::atomic::Ordering;

use super::PluginHost;
use crate::plugin::runtime::manifest::info::PluginInfo;
use crate::plugin::runtime::manifest::permissions::PluginPermissions;
use crate::plugin::runtime::manifest::info::PluginHooks;
use crate::plugin::runtime::manifest::schedule::PluginScheduleStatus;

impl PluginHost {
    /// Return true when a plugin with the given manifest name is currently
    /// loaded AND enabled. Used by sibling plugins (via `arbor.meta.plugin_loaded`)
    /// that need to branch on whether another plugin is active right now,
    /// without relying on the fire-and-forget `arbor.service.call` mechanism
    /// (which races against plugin reload + can silently no-op on lock
    /// poisoning). Returns false for unknown names and dormant entries.
    pub fn is_plugin_enabled(&self, name: &str) -> bool {
        self.plugins.iter().any(|p| p.manifest.name == name && p.is_enabled())
    }

    pub fn list_plugin_info(&self) -> Vec<PluginInfo> {
        // Pre-build a reverse-edge map (dep_name → [dependent names]) so each
        // entry below can answer "who needs me?" in O(1). Both live and
        // dormant plugins contribute edges — load-failed entries don't, since
        // we lost their manifest. Optional edges are excluded so the
        // "Required by" row only lists plugins that would break.
        use std::collections::HashMap;
        let mut required_by_map: HashMap<String, Vec<String>> = HashMap::new();
        for p in &self.plugins {
            for d in &p.manifest.dependencies {
                if d.optional { continue; }
                required_by_map.entry(d.name.clone()).or_default()
                    .push(p.manifest.name.clone());
            }
        }
        for d in &self.dormant {
            for dep in &d.manifest.dependencies {
                if dep.optional { continue; }
                required_by_map.entry(dep.name.clone()).or_default()
                    .push(d.manifest.name.clone());
            }
        }
        for list in required_by_map.values_mut() {
            list.sort();
            list.dedup();
        }

        let mut infos: Vec<PluginInfo> = self.plugins.iter().map(|p| {
            let regs = p.schedules.lock().map(|g| g.clone()).unwrap_or_default();
            let scheduler_count = regs.len();
            let schedules: Vec<PluginScheduleStatus> = regs.into_iter().map(|s| {
                let key = format!("{}:{}", p.manifest.name, s.action);
                let running = self.scheduler_cancels.get(&key)
                    .map(|c| !c.load(Ordering::Relaxed))
                    .unwrap_or(false);
                PluginScheduleStatus { schedule: s, running }
            }).collect();
            let schedulers_running = schedules.iter().filter(|s| s.running).count();
            let doc = p.manifest.doc_file.as_ref()
                .and_then(|f| std::fs::read_to_string(p.manifest.dir.join(f)).ok());
            PluginInfo {
                name:        p.manifest.name.clone(),
                version:     p.manifest.version.clone(),
                description: p.manifest.description.clone(),
                author:      p.manifest.author.clone(),
                license:     p.manifest.license.clone(),
                repository:  p.manifest.repository.clone(),
                keywords:    p.manifest.keywords.clone(),
                arbor_api:   p.manifest.arbor_api,
                enabled:     p.is_enabled(),
                experimental: p.manifest.experimental,
                permissions: p.manifest.permissions.clone(),
                hooks:       p.manifest.hooks.clone(),
                scheduler_count,
                schedulers_running,
                schedules,
                doc,
                dep_error: None,
                dependencies: p.manifest.dependencies.clone(),
                required_by:  required_by_map.get(&p.manifest.name).cloned().unwrap_or_default(),
            }
        }).collect();

        // Append plugins that failed to load due to dependency errors.
        for f in &self.load_failures {
            infos.push(PluginInfo {
                name:               f.name.clone(),
                version:            f.version.clone(),
                description:        f.description.clone(),
                author:             f.author.clone(),
                license:            None,
                repository:         None,
                keywords:           Vec::new(),
                arbor_api:          0,
                enabled:            false,
                experimental:       false,
                permissions:        PluginPermissions::default(),
                hooks:              PluginHooks::default(),
                scheduler_count:    0,
                schedulers_running: 0,
                schedules:          Vec::new(),
                doc:                None,
                dep_error:          Some(f.error.clone()),
                dependencies:       Vec::new(),
                required_by:        required_by_map.get(&f.name).cloned().unwrap_or_default(),
            });
        }

        // Append dormant plugins (disabled at startup, no Lua VM yet).
        for d in &self.dormant {
            let m = &d.manifest;
            let doc = m.doc_file.as_ref()
                .and_then(|f| std::fs::read_to_string(m.dir.join(f)).ok());
            infos.push(PluginInfo {
                name:               m.name.clone(),
                version:             m.version.clone(),
                description:         m.description.clone(),
                author:              m.author.clone(),
                license:             m.license.clone(),
                repository:          m.repository.clone(),
                keywords:            m.keywords.clone(),
                arbor_api:           m.arbor_api,
                enabled:             false,
                experimental:        m.experimental,
                permissions:         m.permissions.clone(),
                hooks:               m.hooks.clone(),
                // Schedules registered via `arbor.scheduler.register` only
                // exist after main.lua has run, so dormant plugins legitimately
                // expose none. Manifest-declared scheduler intent is implicit.
                scheduler_count:    0,
                schedulers_running: 0,
                schedules:          Vec::new(),
                doc,
                dep_error:          None,
                dependencies:       m.dependencies.clone(),
                required_by:        required_by_map.get(&m.name).cloned().unwrap_or_default(),
            });
        }

        infos
    }
}
