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
            });
        }

        infos
    }
}
