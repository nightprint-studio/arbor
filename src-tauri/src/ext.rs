//! Invoking an installed extension, without Arbor knowing what it does.
//!
//! ## The rule this enforces
//!
//! **If the host has to learn something, it is not a plugin.** A feature that needs Arbor to
//! carry its `.wit`, generate bindings, add handlers and grow a panel is a built-in with a
//! wasm file attached — the wasm part is an implementation detail, not an extension point.
//!
//! So there is one entry point here and it is domain-free. A package declares what it
//! provides in `[[provides]]`; whatever functions its module exports become callable, by
//! name, with JSON arguments. Adding a kind of extension — a shader translator, a mesh
//! generator, a format backend — is installing a package. Nothing in this file changes.
//!
//! ## Two callers, one mechanism
//!
//! A plugin calls this through `arbor.ext.*`, and the frontend calls it through the
//! `ext_call` platform handler. They are the same function: a second route with its own
//! implementation is how the two drift, and it is what the cloud surface had before this
//! session removed it.
//!
//! In practice **the plugin decides** — it is the thing that knows which extension to call and
//! what to do with the answer. The frontend's route exists for the case where a node has to
//! fetch a payload too big to carry through a form patch, and it goes through the same gate.
//!
//! ## Addressing
//!
//! By the `[[provides]]` key — interface, version, id — because that is what the registry
//! indexes and what its conflict rules already govern: two packages claiming one id register
//! neither, and that decision should not be re-litigated per call site.
//!
//! ## Cost
//!
//! Every call instantiates. The **compile** is cached by the engine, which is the expensive
//! half; instantiating is cheap and gives each call a fresh store, which is the isolation. A
//! caller that needs a hot instance across many calls is a caller that should be told about
//! it, not one that should be served silently by a shared mutable guest.

use std::sync::Arc;

use arbor_plugin_wasm::prelude::{
    ExtensionEntry, ExtensionIndex, GuestCaps, InterfaceSurface, Services,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// One installed extension, and what it actually exports.
#[derive(Debug, Clone, Serialize)]
pub struct ExtRow {
    /// From `[[provides]]`.
    pub interface: String,
    pub version:   u32,
    pub id:        String,
    /// The package that provides it.
    pub plugin:    String,
    /// What the module exports, read from the component itself rather than from the manifest
    /// — a package that claimed an interface it does not export shows up here as a row with
    /// nothing in it, which is a fact the manifest cannot tell you.
    pub exports:   Vec<InterfaceSurface>,
}

/// How a caller names one call.
#[derive(Debug, Clone, Deserialize)]
pub struct CallSpec {
    pub interface: String,
    #[serde(default = "one")]
    pub version: u32,
    pub id: String,
    /// The function to call.
    pub method: String,
    /// Positional arguments. A component's type information carries parameter types but not
    /// their names, so there is nothing to key an object on.
    #[serde(default)]
    pub args: Vec<Json>,
    /// The exported interface to look the function up in, when the module exports more than
    /// one. Omitted, the one matching `interface` is used.
    #[serde(default)]
    pub export: Option<String>,
}

fn one() -> u32 {
    1
}

fn index() -> Option<(ExtensionIndex, Vec<arbor_plugin_types::prelude::Manifest>)> {
    let manifests = arbor_plugin_core::prelude::discover_plugins().ok()?;
    let enabled = arbor_plugin_core::prelude::load_plugin_states();
    let index = ExtensionIndex::build(&manifests, &enabled);
    Some((index, manifests))
}

/// Build the capability envelope for an entry.
///
/// From the manifest, always. This is the half Arbor is *not* agnostic about: what a guest
/// may reach is Arbor's decision and stays typed and gated, however opaque its exports are.
fn envelope(
    entry: &ExtensionEntry,
    manifests: &[arbor_plugin_types::prelude::Manifest],
) -> Result<(GuestCaps, Services), String> {
    let manifest = manifests
        .iter()
        .find(|m| m.name == entry.plugin)
        .ok_or_else(|| format!("'{}' is no longer installed", entry.plugin))?;
    let caps = GuestCaps::from_manifest(manifest);
    let plugin = entry.plugin.clone();
    let services: Services = Arc::new(crate::plugin_wasm::TauriHostServices::new(Box::new(
        move |_p: &str, level: &str, message: &str| {
            tracing::debug!("[{plugin}] {level}: {message}");
        },
    )));
    Ok((caps, services))
}

/// Every installed extension and the functions it exports.
///
/// A module that will not instantiate is listed with empty `exports` rather than dropped:
/// "installed but broken" and "not installed" lead somewhere different, and a caller that
/// cannot tell them apart tells the user the wrong thing.
pub fn surface() -> Vec<ExtRow> {
    let Some((index, manifests)) = index() else { return Vec::new() };
    let engine = crate::plugin_wasm::engine().ok();

    index
        .all()
        .map(|entry| {
            let exports = engine
                .as_ref()
                .and_then(|host| {
                    let (caps, services) = envelope(entry, &manifests).ok()?;
                    let guest = host
                        .open_dynamic(&entry.module, caps, services)
                        .map_err(|e| {
                            tracing::warn!("extension '{}' would not instantiate: {e}", entry.plugin)
                        })
                        .ok()?;
                    Some(guest.surface(host.engine()))
                })
                .unwrap_or_default();

            ExtRow {
                interface: entry.key.interface.clone(),
                version:   entry.key.version,
                id:        entry.key.id.clone(),
                plugin:    entry.plugin.clone(),
                exports,
            }
        })
        .collect()
}

/// Pick the exported interface a call should look in.
///
/// One export → that one, whatever it is called. Several → the one whose name carries the
/// declared interface, matched on `/<interface>@` so the component model's own
/// `namespace:package/interface@version` spelling is the only convention involved, and no
/// package's choice of namespace is.
fn resolve_export(
    exports: &[InterfaceSurface],
    declared: &str,
    explicit: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(name) = explicit {
        return if exports.iter().any(|e| e.name == name) {
            Ok(Some(name.to_string()))
        } else {
            Err(format!("this extension exports no interface '{name}'"))
        };
    }
    match exports.len() {
        0 => Ok(None),
        1 => Ok(Some(exports[0].name.clone())),
        _ => {
            let needle = format!("/{declared}@");
            exports
                .iter()
                .find(|e| e.name.contains(&needle) || e.name.ends_with(&format!("/{declared}")))
                .map(|e| Some(e.name.clone()))
                .ok_or_else(|| {
                    format!(
                        "this extension exports {} interfaces and none is '{declared}' — name \
                         one of: {}",
                        exports.len(),
                        exports.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
                    )
                })
        }
    }
}

/// Call one function on one extension.
pub fn call(spec: &CallSpec) -> Result<Json, String> {
    let (index, manifests) = index().ok_or("plugins could not be read")?;
    let entry = index
        .resolve(&spec.interface, spec.version, &spec.id)
        .ok_or_else(|| {
            format!(
                "no extension provides {}@{}/{} — check it is installed and enabled",
                spec.interface, spec.version, spec.id
            )
        })?;

    let (caps, services) = envelope(entry, &manifests)?;
    let host = crate::plugin_wasm::engine().map_err(|e| e.to_string())?;
    let mut guest = host
        .open_dynamic(&entry.module, caps, services)
        .map_err(|e| format!("'{}' would not start: {e}", entry.plugin))?;

    let exports = guest.surface(host.engine());
    let export = resolve_export(&exports, &spec.interface, spec.export.as_deref())?;
    guest.call(export.as_deref(), &spec.method, &spec.args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str) -> InterfaceSurface {
        InterfaceSurface { name: name.to_string(), funcs: Vec::new() }
    }

    #[test]
    fn a_lone_export_is_used_whatever_it_is_called() {
        // A package's namespace is its own business; requiring it to match the declared
        // interface would make `[[provides]]` a claim about someone else's naming.
        let e = vec![iface("acme:things/geometry@2.1.0")];
        assert_eq!(
            resolve_export(&e, "mesh-source", None).unwrap(),
            Some("acme:things/geometry@2.1.0".into())
        );
    }

    #[test]
    fn several_exports_are_disambiguated_by_the_declared_interface() {
        let e = vec![
            iface("arbor:extensions/mesh-source@1.0.0"),
            iface("arbor:extensions/shader-preview@1.0.0"),
        ];
        assert_eq!(
            resolve_export(&e, "shader-preview", None).unwrap(),
            Some("arbor:extensions/shader-preview@1.0.0".into())
        );
    }

    #[test]
    fn an_ambiguous_call_names_the_choices_instead_of_guessing() {
        // Picking the first would work until the day a package reorders its exports.
        let e = vec![iface("a:b/one@1.0.0"), iface("a:b/two@1.0.0")];
        let err = resolve_export(&e, "three", None).unwrap_err();
        assert!(err.contains("one@1.0.0") && err.contains("two@1.0.0"), "{err}");
    }

    #[test]
    fn an_explicit_export_has_to_exist() {
        let e = vec![iface("a:b/one@1.0.0")];
        assert_eq!(resolve_export(&e, "one", Some("a:b/one@1.0.0")).unwrap(), Some("a:b/one@1.0.0".into()));
        assert!(resolve_export(&e, "one", Some("a:b/nope@1.0.0")).is_err());
    }

    #[test]
    fn a_module_with_no_interface_exports_falls_back_to_top_level_lookup() {
        // `None` means "look the function up at the top level", which is what a component
        // exporting bare functions needs.
        assert_eq!(resolve_export(&[], "anything", None).unwrap(), None);
    }

    #[test]
    fn a_call_spec_defaults_the_version_to_one() {
        // Version 1 is what every interface starts at, and making every call site write it
        // would be ceremony for the case that is always true today.
        let spec: CallSpec = serde_json::from_value(serde_json::json!({
            "interface": "mesh-source", "id": "fulcrum", "method": "catalogue"
        }))
        .unwrap();
        assert_eq!(spec.version, 1);
        assert!(spec.args.is_empty());
    }
}
