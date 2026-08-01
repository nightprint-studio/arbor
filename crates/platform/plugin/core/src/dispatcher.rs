//! Build the runtime hook dispatcher for an Arbor plugin host.

use std::sync::{Arc, Mutex};

use arbor_plugin_api::prelude::{HookDef, HookDispatcher, HookKind};
use arbor_plugin_types::prelude::{hook_names, hook_ns};

use crate::hook_router::LuaHookListener;
use crate::runtime::PluginHost;

/// Build the [`HookDispatcher`] for a plugin host: register every hook in the
/// static [`HOOK_CATALOG`](arbor_plugin_types::prelude::HOOK_CATALOG), marking
/// the names listed in `vetoable` as [`HookKind::Vetoable`] and everything else
/// fire-and-forget, then bind a single [`LuaHookListener`] to `plugin_host`.
///
/// Parametrised on `vetoable` because that list is the **only** thing that ever
/// differed between the per-product copies of this function: the shell and
/// `corvus-be` mark `on_pre_commit`, every host-pure backend marks nothing. A
/// product that needs the dispatcher therefore reaches for one of the two named
/// wrappers below instead of growing a fourth copy in its own `plugin.rs`.
///
/// Passing the `Arc` rather than a `Weak` is deliberate: the listener has to
/// downgrade it itself, and a builder that ignored the argument would produce a
/// dispatcher with no listener at all — every fire silently doing nothing, which
/// is exactly the failure this signature makes hard to write.
///
/// Note that `kind` is metadata for `arbor.hooks.describe()`, not a gate: whether
/// a fire can be vetoed is decided at the call site (`fire_vetoable_blocking` vs
/// `fire_blocking`), so a host that registers a hook fire-and-forget and then
/// fires it vetoably still collects the veto. Keep the two in agreement anyway —
/// the catalog is what a plugin author reads.
pub fn build_hook_dispatcher_with(
    plugin_host: &Arc<Mutex<PluginHost>>,
    vetoable: &[&str],
) -> HookDispatcher {
    build_hook_dispatcher_inner(plugin_host, None, vetoable)
}

/// Like [`build_hook_dispatcher_with`], but registering only the hooks in
/// `namespaces`.
///
/// The reason to prefer it: a backend that registers the whole catalog answers
/// `arbor.hooks.list()` with hooks it can never fire, so a plugin author reads
/// `garrulus:note_saved` in the file explorer's introspection and writes a
/// handler that will never run. Registering a product's own namespace plus
/// [`hook_names::arbor`] (the lifecycle hooks every host fires) tells the truth.
pub fn build_hook_dispatcher_for_ns(
    plugin_host: &Arc<Mutex<PluginHost>>,
    namespaces: &[&str],
    vetoable: &[&str],
) -> HookDispatcher {
    build_hook_dispatcher_inner(plugin_host, Some(namespaces), vetoable)
}

fn build_hook_dispatcher_inner(
    plugin_host: &Arc<Mutex<PluginHost>>,
    namespaces: Option<&[&str]>,
    vetoable: &[&str],
) -> HookDispatcher {
    let mut dispatcher = HookDispatcher::new();
    for h in arbor_plugin_types::prelude::HOOK_CATALOG {
        if let Some(allowed) = namespaces {
            if !allowed.iter().any(|ns| hook_ns::is_in_ns(h.name, ns)) {
                continue;
            }
        }
        dispatcher.register_hook(HookDef {
            name:        h.name,
            category:    h.category,
            description: h.description,
            kind:        if vetoable.iter().any(|v| *v == h.name) {
                HookKind::Vetoable
            } else {
                HookKind::FireAndForget
            },
            ctx: h.ctx,
        });
    }
    dispatcher.register_listener(Arc::new(LuaHookListener::new(Arc::downgrade(plugin_host))));
    dispatcher
}

/// The corvus/shell dispatcher shape: `corvus:pre_commit` is the one vetoable
/// hook.
///
/// Product-agnostic in everything else — the launcher's in-process host and the
/// headless `corvus-be` host both build through here, so a fire from either side
/// fans out identically.
pub fn build_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
    build_hook_dispatcher_with(plugin_host, &[hook_names::corvus::PRE_COMMIT])
}

/// The host-pure dispatcher shape: every catalog hook is fire-and-forget.
///
/// What `sitta-be` / `tyto-be` / `garrulus-be` want. They register the *whole*
/// catalog even though most of it is corvus's: hooks that a product never fires
/// simply never arrive, and registering them keeps `arbor.hooks.describe()`
/// answering the same thing in every backend.
pub fn host_pure_hook_dispatcher(plugin_host: &Arc<Mutex<PluginHost>>) -> HookDispatcher {
    build_hook_dispatcher_with(plugin_host, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Arc<Mutex<PluginHost>> {
        Arc::new(Mutex::new(PluginHost::new()))
    }

    /// Every catalog entry must reach the dispatcher — `describe()` answers from
    /// this table, so a dropped entry is a hook that documents itself as absent.
    #[test]
    fn registers_the_whole_catalog() {
        let h = host();
        let d = build_hook_dispatcher(&h);
        let registered = d.iter_hooks().count();
        assert_eq!(registered, arbor_plugin_types::prelude::HOOK_CATALOG.len());
    }

    #[test]
    fn marks_only_the_requested_names_vetoable() {
        let h = host();
        let d = build_hook_dispatcher_with(&h, &[hook_names::corvus::PRE_COMMIT]);
        assert!(matches!(
            d.lookup(hook_names::corvus::PRE_COMMIT).map(|x| x.kind),
            Some(HookKind::Vetoable)
        ));
        assert!(matches!(
            d.lookup(hook_names::corvus::COMMIT).map(|x| x.kind),
            Some(HookKind::FireAndForget)
        ));
    }

    #[test]
    fn host_pure_marks_nothing_vetoable() {
        let h = host();
        let d = host_pure_hook_dispatcher(&h);
        assert!(d.iter_hooks().all(|x| matches!(x.kind, HookKind::FireAndForget)));
    }

    /// A name that is not in the catalog must not silently invent an entry — the
    /// caller mistyped, and `describe()` should keep telling the truth.
    #[test]
    fn unknown_vetoable_name_is_ignored() {
        let h = host();
        let d = build_hook_dispatcher_with(&h, &["corvus:pre_commmit"]);
        assert!(d.iter_hooks().all(|x| matches!(x.kind, HookKind::FireAndForget)));
        assert!(d.lookup("corvus:pre_commmit").is_none());
    }

    /// A namespace-scoped dispatcher must not advertise another product's hooks.
    #[test]
    fn ns_filtered_dispatcher_registers_only_those_namespaces() {
        let h = host();
        let d = build_hook_dispatcher_for_ns(
            &h,
            &[hook_names::arbor::NS, hook_names::garrulus::NS],
            &[],
        );
        assert!(d.lookup(hook_names::garrulus::NOTE_SAVED).is_some());
        assert!(d.lookup(hook_names::arbor::PLUGIN_LOAD).is_some());
        assert!(d.lookup(hook_names::corvus::COMMIT).is_none());
        assert_eq!(
            d.iter_hooks().count(),
            hook_names::arbor::ALL.len() + hook_names::garrulus::ALL.len()
        );
    }
}
