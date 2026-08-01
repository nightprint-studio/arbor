//! Dynamic hook definitions — populated at boot by every crate that fires
//! hooks (rather than baked into a static catalog like
//! `arbor_plugin_types::hook_catalog::HOOK_CATALOG`).
//!
//! The static catalog stays where it is (one cross-domain list, shipped by
//! the types crate, consumed by the docs panel and by the marketplace). The
//! dynamic catalog is the [`crate::registry::PluginRegistry`]'s `hooks` table:
//! each domain contributes its [`HookDef`]s through a
//! [`crate::namespace::NamespaceContributor`].
//!
//! Decision **D7**: vetoable hooks are flagged on the definition itself; the
//! dispatcher exposes a separate `fire_vetoable` entry point so the call site
//! is type-safe instead of stringly-typed.
//!
//! Decision **D9/D10**: `name` is always a `<namespace>:<event>` constant from
//! `arbor_plugin_types::prelude::hook_names`, never a literal written here. A
//! definition registered under a name nothing fires is invisible, and a
//! definition *missing* for a name something fires makes the runtime treat a
//! real hook as a plugin-defined event — both silent, which is why the name
//! has exactly one source.

use arbor_plugin_types::prelude::{hook_ns, HookField};

/// Whether a hook accepts a veto from its handlers.
///
/// A vetoable hook lets a listener abort the in-flight action by returning a
/// non-empty reason string; only `on_pre_commit` uses this convention today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    /// All listeners are invoked; their return values are ignored.
    FireAndForget,
    /// The first listener to return a non-empty reason aborts the action and
    /// the remaining listeners are skipped.
    Vetoable,
}

/// A hook contributed by a domain crate.
///
/// Mirrors [`arbor_plugin_types::prelude::HookDef`] but adds [`HookKind`] so
/// the runtime knows whether to expect a veto. `ctx` is the schema of the
/// payload table the handler receives — same shape as the static catalog so
/// the docs renderer can treat both lists uniformly.
#[derive(Debug, Clone)]
pub struct HookDef {
    pub name:        &'static str,
    pub category:    &'static str,
    pub description: &'static str,
    pub kind:        HookKind,
    pub ctx:         &'static [HookField],
}

impl HookDef {
    /// The namespace half of [`HookDef::name`] (`"corvus"` for
    /// `"corvus:commit"`).
    ///
    /// `None` only for a definition contributed under an unqualified name,
    /// which no built-in does — a domain crate that hits this is registering a
    /// name it built by hand instead of through `hook_name!`.
    pub fn namespace(&self) -> Option<&'static str> {
        hook_ns::namespace_of(self.name)
    }

    /// The event half of [`HookDef::name`], without the namespace.
    pub fn event(&self) -> &'static str {
        hook_ns::event_of(self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_plugin_types::prelude::hook_names::corvus;

    fn def(name: &'static str) -> HookDef {
        HookDef {
            name,
            category: "git",
            description: "",
            kind: HookKind::FireAndForget,
            ctx: &[],
        }
    }

    #[test]
    fn splits_a_namespaced_name() {
        let d = def(corvus::COMMIT);
        assert_eq!(d.namespace(), Some("corvus"));
        assert_eq!(d.event(), "commit");
    }

    #[test]
    fn an_unqualified_name_has_no_namespace() {
        let d = def("commit");
        assert_eq!(d.namespace(), None);
        assert_eq!(d.event(), "commit");
    }
}
