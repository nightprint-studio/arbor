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
//! implementation is how the two drift, and it is what the extension surface had before this
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
//! half; instantiating is cheap and gives each call a fresh store, which is the isolation.
//!
//! ## Unless the package asks to stay up
//!
//! `persistent = true` on a `[[provides]]` keeps **one** instance alive across calls, so a guest
//! can keep work between them — a language backend keeps its index and re-reads only the files
//! that changed, instead of rebuilding a whole project on every keystroke. Opt-in, and declared
//! by the package, because a guest written for a fresh store may hold state it never expected to
//! outlive a call: serving it silently from a shared mutable instance would change what it means.
//!
//! The live instance is the host's to drop, and the guest must answer correctly from a fresh one:
//! it goes when the plugin set moves ([`forget_compiled`]), when a call fails (a trap can leave a
//! store half-written, and telling a trap from the guest's own `Err` is not worth the risk), and
//! when it has sat idle for [`IDLE`]. Calls to one instance are serialised by its lock.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arbor_plugin_wasm::prelude::{
    DynGuest, ExtensionEntry, ExtensionIndex, GuestCaps, InterfaceSurface, Services,
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

/// Where the bytes of a byte-shaped call come from, or go.
///
/// Paired with a [`CallSpec`] by [`call_to_file`] / [`call_from_file`]. The path arrives
/// already absolute and already checked against the calling plugin's `fs` permission — that
/// check belongs to the plugin's own context, which lives in the backend, so it happens there
/// and this side does not repeat it.
#[derive(Debug, Clone, Deserialize)]
pub struct FileSpec {
    /// Absolute path of the local file.
    pub path: String,
    /// Append rather than truncate (`call_to_file`). A chunked download is a sequence of ranged
    /// reads appended in order, so this is the normal case after the first chunk.
    #[serde(default)]
    pub append: bool,
    /// Which positional argument the file's bytes are lowered into (`call_from_file`), 1-based
    /// to match Lua's own indexing. Whatever `args` holds at that position is ignored.
    #[serde(default)]
    pub file_arg: usize,
    /// Read from this offset (`call_from_file`). Lets an upload be chunked without the caller
    /// holding the file.
    #[serde(default)]
    pub offset: u64,
    /// Read at most this many bytes; `0` means to the end.
    #[serde(default)]
    pub length: u64,
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

/// The repository open in Arbor right now — what a guest's `fs` gate treats as its sandbox.
///
/// The shell's cached active-tab path, the same value the Lua host seeds `__arbor_current_repo__`
/// from. Read here, per call, and handed to the guest by the caller: a guest cannot name it, and a
/// persistent one is never judged against the repository it was created under.
pub fn active_repo(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let path = state.active_repo_path.lock().ok()?.clone()?;
    Some(PathBuf::from(path))
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

/// Bring up the guest an entry names, and resolve which of its exports to look in.
fn open_entry(
    entry: &ExtensionEntry,
    manifests: &[arbor_plugin_types::prelude::Manifest],
    spec: &CallSpec,
) -> Result<(DynGuest, Option<String>), String> {
    let (caps, services) = envelope(entry, manifests)?;
    let host = crate::plugin_wasm::engine().map_err(|e| e.to_string())?;
    let guest = host
        .open_dynamic(&entry.module, caps, services)
        .map_err(|e| format!("'{}' would not start: {e}", entry.plugin))?;

    let exports = guest.surface(host.engine());
    let export = resolve_export(&exports, &spec.interface, spec.export.as_deref())?;
    Ok((guest, export))
}

/// Run `f` on the guest a spec addresses: a fresh one, or the live one of a persistent package.
///
/// The three entry points below differ only in what they do with the call's bytes; getting to
/// the call is the same every time, and a second copy of it would be a second place for the
/// "not installed / would not start / which export" messages to drift.
fn with_guest<R>(
    spec: &CallSpec,
    repo: Option<PathBuf>,
    f: impl FnOnce(&mut DynGuest, Option<&str>) -> Result<R, String>,
) -> Result<R, String> {
    let (index, manifests) = index().ok_or("plugins could not be read")?;
    let entry = index
        .resolve(&spec.interface, spec.version, &spec.id)
        .ok_or_else(|| {
            format!(
                "no extension provides {}@{}/{} — check it is installed and enabled",
                spec.interface, spec.version, spec.id
            )
        })?;
    if !entry.persistent {
        let (mut guest, export) = open_entry(entry, &manifests, spec)?;
        guest.set_active_repo(repo);
        return f(&mut guest, export.as_deref());
    }

    let key = live_key(entry, spec);
    let live = match live_get(&key) {
        Some(l) => l,
        // Brought up outside the table's lock: instantiating takes milliseconds, and every other
        // extension call would wait on it. Two callers racing here each build one; the second
        // insert wins and the first instance serves its one call and goes.
        None => {
            let (guest, export) = open_entry(entry, &manifests, spec)?;
            let l = Arc::new(Live { slot: Mutex::new((guest, export)), last_used: Mutex::new(Instant::now()) });
            live_table().lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone(), l.clone());
            l
        }
    };
    let result = {
        let mut slot = live.slot.lock().unwrap_or_else(|e| e.into_inner());
        let (guest, export) = &mut *slot;
        // Before every call, not only when the instance is created: a live guest outlives tab
        // switches, and its `fs` gate must follow the repository that is open now.
        guest.set_active_repo(repo);
        f(guest, export.as_deref())
    };
    *live.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    if result.is_err() {
        live_table().lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
    }
    result
}

/// Call one function on one extension.
pub fn call(spec: &CallSpec, repo: Option<PathBuf>) -> Result<Json, String> {
    with_guest(spec, repo, |guest, export| guest.call(export, &spec.method, &spec.args))
}

/// Call one function and write its bytes to a local file. Returns how many were written.
///
/// Why this exists rather than the caller doing it: the answer is a blob, and the way back to
/// a plugin is JSON. A megabyte of object becomes six megabytes of number-array to serialise,
/// parse and hold — once in each process it passes through. Here the bytes go from the guest
/// to the file and are never a document.
///
/// It is also what keeps the host out of the domain. Arbor is not learning what a download is;
/// it is writing the result of a call it knows nothing about into a path the caller named.
pub fn call_to_file(spec: &CallSpec, file: &FileSpec, repo: Option<PathBuf>) -> Result<u64, String> {
    use std::io::Write;

    let bytes = with_guest(spec, repo, |guest, export| guest.call_to_bytes(export, &spec.method, &spec.args))?;

    let path = std::path::Path::new(&file.path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(file.append)
        .truncate(!file.append)
        .open(path)
        .map_err(|e| format!("arbor.ext.call_to_file {}: {e}", file.path))?;
    out.write_all(&bytes)
        .map_err(|e| format!("arbor.ext.call_to_file {}: {e}", file.path))?;
    Ok(bytes.len() as u64)
}

/// Call one function passing the contents of a local file as one of its arguments.
///
/// The upload direction of [`call_to_file`], and the same reasoning: an argument that is a blob
/// has no business being a JSON array. `offset` / `length` are what make a chunked upload
/// possible without the caller holding the whole file.
pub fn call_from_file(spec: &CallSpec, file: &FileSpec, repo: Option<PathBuf>) -> Result<Json, String> {
    use std::io::{Read, Seek, SeekFrom};

    let bytes_at = byte_arg_index(file.file_arg, spec.args.len())?;

    let mut f = std::fs::File::open(&file.path)
        .map_err(|e| format!("arbor.ext.call_from_file {}: {e}", file.path))?;
    if file.offset > 0 {
        f.seek(SeekFrom::Start(file.offset))
            .map_err(|e| format!("arbor.ext.call_from_file {}: {e}", file.path))?;
    }
    let mut bytes = Vec::new();
    let read = if file.length > 0 {
        f.take(file.length).read_to_end(&mut bytes)
    } else {
        f.read_to_end(&mut bytes)
    };
    read.map_err(|e| format!("arbor.ext.call_from_file {}: {e}", file.path))?;

    with_guest(spec, repo, |guest, export| guest.call_with_bytes(export, &spec.method, &spec.args, bytes_at, &bytes))
}

// ── live instances (`persistent = true`) ─────────────────────────────────────────

/// How long a live instance may sit unused before it is dropped. Long enough to span a pause in
/// typing; short enough that a package nobody is using does not keep its memory forever.
pub(crate) const IDLE: Duration = Duration::from_secs(10 * 60);

/// One live instance and when it last answered.
struct Live {
    slot: Mutex<(DynGuest, Option<String>)>,
    last_used: Mutex<Instant>,
}

fn live_table() -> &'static Mutex<HashMap<String, Arc<Live>>> {
    static LIVE: OnceLock<Mutex<HashMap<String, Arc<Live>>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The key is the provider **and the module path**: a package moved or reinstalled elsewhere is
/// a different instance, never the old one answering for the new file.
fn live_key(entry: &ExtensionEntry, spec: &CallSpec) -> String {
    let module: &PathBuf = &entry.module;
    format!("{}|{}|{}", entry.key, module.display(), spec.export.as_deref().unwrap_or(""))
}

/// The live instance for `key`, after dropping the ones that sat idle too long.
fn live_get(key: &str) -> Option<Arc<Live>> {
    let mut table = live_table().lock().unwrap_or_else(|e| e.into_inner());
    table.retain(|_, l| l.last_used.lock().map(|t| t.elapsed() < IDLE).unwrap_or(false));
    table.get(key).cloned()
}

/// Drop every live instance. Called with [`forget_compiled`]: a module rebuilt on disk must not
/// be answered for by the instance of the previous build.
pub fn forget_live() {
    live_table().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Turn the caller's 1-based `file_arg` into the 0-based position the bridge wants.
///
/// 1-based on the way in because the caller is writing Lua, where every index is; converted
/// here, once, rather than at the call site where an off-by-one would surface as bytes landing
/// in the wrong parameter.
fn byte_arg_index(file_arg: usize, argc: usize) -> Result<usize, String> {
    if file_arg == 0 || file_arg > argc {
        return Err(format!(
            "arbor.ext.call_from_file: `file_arg` must name one of the {argc} argument(s) (1-based)"
        ));
    }
    Ok(file_arg - 1)
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
    fn a_byte_argument_is_named_the_way_lua_counts() {
        assert_eq!(byte_arg_index(1, 3).unwrap(), 0);
        assert_eq!(byte_arg_index(3, 3).unwrap(), 2);
    }

    #[test]
    fn a_byte_argument_outside_the_call_is_refused() {
        // Zero is the give-away that somebody wrote a 0-based index; past the end is a spec
        // that no longer matches the call it was written for. Both would otherwise reach the
        // bridge as a position it cannot check against anything.
        assert!(byte_arg_index(0, 2).is_err());
        assert!(byte_arg_index(3, 2).is_err());
    }

    #[test]
    fn a_file_spec_defaults_to_truncating_a_whole_file() {
        // Every field optional: the common call is "write the answer here", and the chunked
        // variants are the ones that say more.
        let file: FileSpec = serde_json::from_value(serde_json::json!({ "path": "/tmp/x" })).unwrap();
        assert!(!file.append);
        assert_eq!((file.file_arg, file.offset, file.length), (0, 0, 0));
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
