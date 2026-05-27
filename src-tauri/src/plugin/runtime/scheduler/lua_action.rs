//! `Action` impl that drives a Lua-registered plugin hook from the shared
//! `arbor-scheduler` engine.
//!
//! Held by the engine as an `Arc<dyn Action>`; upgrades the weak
//! self-reference on every fire so a host swap / unload doesn't keep the
//! task alive past its useful life.

use std::sync::{Mutex, Weak};

use arbor_scheduler::prelude::Action;
use async_trait::async_trait;

use crate::plugin::runtime::host::PluginHost;

pub(crate) struct LuaHookAction {
    pub host:        Weak<Mutex<PluginHost>>,
    pub plugin_name: String,
    pub action_name: String,
}

#[async_trait]
impl Action for LuaHookAction {
    async fn fire(&self) {
        let Some(host_arc) = self.host.upgrade() else {
            tracing::debug!(
                "scheduler fire for '{}:{}' arrived after host drop — skipping",
                self.plugin_name, self.action_name
            );
            return;
        };
        let plugin = self.plugin_name.clone();
        let action = self.action_name.clone();

        // Lua handlers are synchronous and can take arbitrary user time —
        // hop onto a blocking-pool thread so the executor isn't pinned.
        let join = tokio::task::spawn_blocking(move || {
            match host_arc.lock() {
                Ok(host) => host.fire_hook_on(&plugin, &action, "{}"),
                Err(e)   => {
                    tracing::warn!(
                        "plugin_host mutex poisoned in scheduler fire for '{plugin}:{action}': {e}"
                    );
                    Ok(())
                }
            }
        }).await;
        if let Err(e) = join {
            tracing::warn!(
                "scheduler fire join error for '{}:{}': {e}",
                self.plugin_name, self.action_name
            );
        }
    }
}
