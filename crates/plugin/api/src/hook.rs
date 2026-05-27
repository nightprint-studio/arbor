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

use arbor_plugin_types::prelude::HookField;

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
