//! `PluginRegistry` — the collector populated at boot.
//!
//! Each domain crate's [`crate::namespace::NamespaceContributor`] dumps its
//! namespaces, hooks and permission keys here. The runtime adapter
//! (`LuaRuntime`, later `WasmRuntime`) reads the populated registry to
//! materialise the script-side `arbor.<namespace>.<fn>` tables, and calls
//! [`PluginRegistry::invoke`] every time a script calls a contributed function.
//!
//! Decision **D8** (the dispatcher half) lives in [`crate::dispatcher`]; the
//! registry stores hook *definitions* (metadata for the docs panel and for
//! validation) but does not fire them.

use std::collections::{BTreeMap, HashMap};

use arbor_plugin_types::prelude::Manifest;

use crate::ctx::PluginCtx;
use crate::error::PluginError;
use crate::func::NamespaceFn;
use crate::hook::HookDef;
use crate::perm::{ManifestPermError, PermSchema, PermissionDef, PermissionsView};
use crate::value::PluginValue;

/// The central registry — namespaces of functions, hook definitions,
/// permission keys — built once at app boot, then read-only at runtime.
pub struct PluginRegistry {
    namespaces:  HashMap<&'static str, BTreeMap<&'static str, NamespaceFn>>,
    permissions: HashMap<&'static str, PermissionDef>,
    hooks:       HashMap<&'static str, HookDef>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            namespaces:  HashMap::new(),
            permissions: HashMap::new(),
            hooks:       HashMap::new(),
        }
    }

    /// Register a function under `(f.namespace, f.name)`.
    ///
    /// Panics on duplicate `(namespace, name)` pairs — registration is a boot
    /// step, a duplicate there is a programmer error and should fail loud.
    pub fn register_fn(&mut self, f: NamespaceFn) {
        let ns = self.namespaces.entry(f.namespace).or_default();
        if ns.contains_key(f.name) {
            panic!(
                "arbor-plugin-api: duplicate function '{}.{}'",
                f.namespace, f.name
            );
        }
        ns.insert(f.name, f);
    }

    /// Register a hook definition. Panics on duplicate names.
    pub fn register_hook(&mut self, h: HookDef) {
        if self.hooks.contains_key(h.name) {
            panic!("arbor-plugin-api: duplicate hook '{}'", h.name);
        }
        self.hooks.insert(h.name, h);
    }

    /// Register a permission key. Panics on duplicate keys.
    pub fn register_permission(&mut self, p: PermissionDef) {
        if self.permissions.contains_key(p.key) {
            panic!("arbor-plugin-api: duplicate permission '{}'", p.key);
        }
        self.permissions.insert(p.key, p);
    }

    /// Look up a registered function. Returns `None` if either the namespace
    /// or the name is unknown.
    pub fn lookup_fn(&self, ns: &str, name: &str) -> Option<&NamespaceFn> {
        self.namespaces.get(ns).and_then(|m| m.get(name))
    }

    /// Iterate every registered hook. Order is unspecified — callers that
    /// render docs typically group by `category` themselves.
    pub fn iter_hooks(&self) -> impl Iterator<Item = &HookDef> {
        self.hooks.values()
    }

    /// Iterate every registered permission key.
    pub fn iter_permissions(&self) -> impl Iterator<Item = &PermissionDef> {
        self.permissions.values()
    }

    /// Iterate every registered function, namespace-by-namespace.
    pub fn iter_fns(&self) -> impl Iterator<Item = (&'static str, &NamespaceFn)> {
        self.namespaces
            .iter()
            .flat_map(|(ns, m)| m.values().map(move |f| (*ns, f)))
    }

    /// Validate a plugin manifest against the registered permission schemas.
    ///
    /// Today this is a no-op (returns `Ok(())`): the `ext: HashMap` extension
    /// to `Permissions` lands in PR #4 — at which point this method will walk
    /// `m.permissions.ext`, look each key up, and run [`PermSchema::validate`]
    /// against the value.
    pub fn validate_manifest(&self, _m: &Manifest) -> Result<(), Vec<ManifestPermError>> {
        Ok(())
    }

    /// Invoke a contributed function with the permission gate in front.
    ///
    /// Steps:
    /// 1. Look up `(ns, name)`. [`PluginError::NotFound`] if missing.
    /// 2. Build a [`PermissionsView`] joining `ctx`'s permissions with the
    ///    registry's schemas (needed for [`crate::perm::PermReq::AtLeast`]
    ///    ordering).
    /// 3. Check every entry in `requires`. The first failure short-circuits.
    /// 4. Await the body.
    pub async fn invoke(
        &self,
        ctx:  &(dyn PluginCtx + Sync),
        ns:   &str,
        name: &str,
        args: PluginValue,
    ) -> Result<PluginValue, PluginError> {
        let f = self
            .lookup_fn(ns, name)
            .ok_or_else(|| PluginError::not_found(format!("{ns}.{name}")))?;

        let view = CtxPermView {
            ctx,
            registry: self,
        };
        for req in f.requires {
            req.check(&view)?;
        }

        f.body.call(ctx, args).await
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter that glues `ctx.permission(key)` with the registry's enum schemas
/// so [`crate::perm::PermReq::AtLeast`] can compare values by index.
///
/// Built fresh per invocation — the registry is `&self` so cheap clones.
struct CtxPermView<'a> {
    ctx:      &'a (dyn PluginCtx + Sync),
    registry: &'a PluginRegistry,
}

impl<'a> PermissionsView for CtxPermView<'a> {
    fn get(&self, key: &str) -> Option<&toml::Value> {
        self.ctx
            .permission(key)
            .or_else(|| self.registry.permissions.get(key).map(|d| &d.default))
    }

    fn at_least(&self, key: &str, required: &str) -> bool {
        let Some(def) = self.registry.permissions.get(key) else {
            return false;
        };
        let PermSchema::Enum(options) = &def.schema else {
            return false;
        };
        let current = self
            .ctx
            .permission(key)
            .or(Some(&def.default))
            .and_then(|v| match v {
                toml::Value::String(s) => Some(s.as_str()),
                _ => None,
            });
        let Some(current) = current else { return false };

        let req_idx = options.iter().position(|o| *o == required);
        let cur_idx = options.iter().position(|o| *o == current);
        matches!((req_idx, cur_idx), (Some(r), Some(c)) if c >= r)
    }
}
