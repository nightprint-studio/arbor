//! `PluginCtx` — what a contributed function sees about its caller.
//!
//! Methods are kept sync (no `async fn`) so the trait stays object-safe and can
//! be passed around as `&(dyn PluginCtx + Sync)`. Anything that needs to be
//! async — say, emitting on a transport that buffers — should do its async
//! work *outside* the ctx, and the ctx itself stays cheap.
//!
//! The host crate (`arbor-plugin-core`) will provide the concrete impl that
//! wires `tauri::Emitter` for app-level events and reads the `Permissions`
//! struct loaded from `plugin.toml`.

use arbor_plugin_types::prelude::Manifest;

use crate::value::PluginValue;

/// View into the calling plugin's identity, manifest, permission table, and
/// the host's event channel.
pub trait PluginCtx: Send + Sync {
    /// Plugin name (the `[plugin].name` from `plugin.toml`).
    fn plugin_name(&self) -> &str;

    /// Full parsed manifest. Lets the function reach typed sections
    /// (sandbox, schedule, dependencies, …) without re-parsing.
    fn manifest(&self) -> &Manifest;

    /// Raw value for a permission key, either typed core (`fs`, `terminal`,
    /// …) or crate-contributed `ext`. `None` if the plugin did not declare it.
    fn permission(&self, key: &str) -> Option<&toml::Value>;

    /// Emit an app-level event. The transport (Tauri's emitter today, maybe
    /// something else tomorrow) is the host's concern — domain crates just
    /// shove the payload down.
    fn emit_app(&self, event: &str, payload: PluginValue);
}
