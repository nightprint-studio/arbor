//! The plugin-host wiring a **host-pure** product backend needs, ready made.
//!
//! "Host-pure" means: the product publishes no `arbor.*` namespace of its own,
//! so its plugins see exactly the namespaces this crate hardcodes (log / events /
//! json / text / fs / http / meta / settings / timer / scheduler / hooks /
//! contribution / keybinding / command / notify / the studios), and it fires no
//! vetoable hook. `sitta-be`, `tyto-be` and `garrulus-be` are all that shape.
//!
//! Before this module each of them carried its own `plugin.rs` holding a copy of
//! the same two items, differing only in the type name — the third copy already
//! carried a comment asking the fourth not to happen. The dispatcher half lives
//! next door in [`crate::dispatcher`] (it is a special case of the parametrised
//! builder); this module owns the installer half and re-exports nothing, so a
//! backend imports both from the prelude and keeps no `plugin.rs` at all.
//!
//! A product that later grows its own namespaces stops being host-pure: it writes
//! a real installer that passes its `LuaNamespaceInstaller`s as `extra`, the way
//! corvus does. That is a deliberate graduation, not a fork of this file.

use std::sync::Arc;

use mlua::Lua;

use crate::error::Result as PluginCoreResult;
use crate::lua_api::{register as register_lua_api, LuaNamespaceInstaller};
use crate::sandbox::{ApiInstallParams, LuaApiInstaller};

/// Publishes only the host-pure `arbor.*` namespaces — the `extra` installer list
/// is empty.
///
/// Stateless on purpose: one value serves every VM the host builds.
pub struct HostPureApiInstaller;

impl LuaApiInstaller for HostPureApiInstaller {
    fn install(&self, lua: &Lua, params: ApiInstallParams) -> PluginCoreResult<()> {
        let no_extra: Vec<Arc<dyn LuaNamespaceInstaller>> = Vec::new();
        register_lua_api(lua, params, &no_extra)
    }
}

/// The host-pure installer as a trait object, so a product `main` wires it
/// without naming `mlua` (and therefore without a direct `mlua` dependency of its
/// own, which would have to be workspace-pinned to stay the same `Lua` type).
///
/// Wiring an installer is not optional: with none set the host substitutes a
/// no-op stub, plugins load, `arbor.*` is simply absent, and `arbor.events.on`
/// never exists — so nothing ever subscribes and every hook fire is silent.
pub fn host_pure_api_installer() -> Arc<dyn LuaApiInstaller> {
    Arc::new(HostPureApiInstaller)
}
