//! One way to say **"this plugin failed"**.
//!
//! A plugin failure has two audiences and they are not the same person. The dev console
//! (`tracing`) is read while attached to a terminal; the **Plugin Logs panel** is read by whoever
//! is writing the plugin, from inside the app, usually long after the terminal scrolled away — and
//! in a Model-D backend there is no terminal at all, because no `*-be` installs a subscriber. A
//! site that logs to only one of the two is silent for the audience that needed it.
//!
//! So both are one call. [`PluginReporter`] carries the two things a report needs — the plugin it
//! belongs to and the host handle that owns the panel's buffer — and is cheap to clone into a Lua
//! closure or a spawned thread, which is what [`crate::lua_ctx`]'s `&Lua`-bound variant cannot do.
//!
//! ```ignore
//! let reporter = ctx.reporter();               // from an `ApiCtx`, at install time
//! reporter.error(format!("arbor.command.fire('{id}'): {e}"));
//! ```
//!
//! The message is written **once** and reaches both places: the console line is prefixed with
//! `[plugin]` for grep, the panel entry is attributed structurally. Sites used to write it twice
//! and the two drifted.

use std::sync::Arc;

use arbor_core::prelude::AppCtx;

/// Who to blame and where to say it.
///
/// `app_ctx` is optional because a plugin can be loaded without a host — unit tests, the
/// standalone `load_plugin` helper — and a report there is a no-op rather than a panic.
#[derive(Clone)]
pub struct PluginReporter {
    plugin: String,
    app_ctx: Option<Arc<dyn AppCtx>>,
}

impl PluginReporter {
    pub fn new(plugin: impl Into<String>, app_ctx: Option<Arc<dyn AppCtx>>) -> Self {
        Self { plugin: plugin.into(), app_ctx }
    }

    /// The plugin every message from this reporter is attributed to.
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// Something the plugin asked for did not happen.
    pub fn error(&self, message: impl Into<String>) {
        self.say("error", message.into());
    }

    /// Something the plugin asked for happened, but not the way it meant it to — a deprecated
    /// shape, a subscription nothing will deliver to, a value that had to be coerced.
    pub fn warn(&self, message: impl Into<String>) {
        self.say("warn", message.into());
    }

    pub fn info(&self, message: impl Into<String>) {
        self.say("info", message.into());
    }

    fn say(&self, level: &str, message: String) {
        let plugin = &self.plugin;
        match level {
            "error" => tracing::error!(target: "plugin", "[{plugin}] {message}"),
            "warn" => tracing::warn!(target: "plugin", "[{plugin}] {message}"),
            "debug" => tracing::debug!(target: "plugin", "[{plugin}] {message}"),
            _ => tracing::info!(target: "plugin", "[{plugin}] {message}"),
        }
        if let Some(ctx) = &self.app_ctx {
            ctx.record_plugin_log(level, plugin, &message);
        }
    }
}
