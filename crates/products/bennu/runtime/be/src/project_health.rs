//! `project_health` domain — the states in which a project **stops being what was opened** without
//! anything saying so: its root directory vanished, a module its reactor declares is gone, or a file
//! on screen belongs to no indexed project at all.
//!
//! Each of these used to be silent. A vanished root kept its slot and answered nothing; a renamed
//! module dropped out of the reactor with every type in it unresolved; a file outside every project
//! got syntax checks only, indistinguishable from a project whose semantics simply found no problem.
//!
//! Handlers: `bennu_file_owner`, `bennu_root_status`. Events: [`EVT_PROJECT_MISSING`],
//! [`EVT_MODULES_MISSING`] — both driven by the classpath watcher's tick and, for modules, by every
//! index build.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::index_service::IndexService;
use crate::project_locate::{enclosing_maven_root, nearest_existing_dir};

/// An open project's root directory is gone. Payload `{ root, nearest_existing }`. The backend has
/// already dropped the project's index; the frontend offers to locate or remove it.
pub const EVT_PROJECT_MISSING: &str = "arbor://bennu/project-missing";

/// A reactor declares modules the disk does not have. Payload
/// `{ root, modules: [{ name, pom }], unlisted: [dir] }`. Emitted once per episode — see [`ANNOUNCED`].
pub const EVT_MODULES_MISSING: &str = "arbor://bennu/modules-missing";

/// Per root, the missing-module set last announced (as a signature). An episode ends when the set
/// changes — including to empty, so a module that goes missing again later is announced again.
static ANNOUNCED: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// One pass over every open project: drop the ones whose root vanished, announce broken reactors.
/// Called from the classpath watcher's tick, so it costs one `stat` per root plus one pom read per
/// module.
pub fn check_open_roots(service: &'static IndexService) {
    let Some(sink) = service.sink() else { return };
    for root in service.open_roots() {
        if Path::new(&root).is_dir() {
            announce_missing_modules(&root, &sink);
        } else {
            forget_missing_root(service, &root, &sink);
        }
    }
}

/// Release a project whose root is gone and tell the frontend. Forgetting is what makes this fire
/// once: the next tick no longer sees the slot.
fn forget_missing_root(service: &IndexService, root: &str, sink: &Arc<dyn EventSink>) {
    eprintln!("bennu-be: project root {root} no longer exists — releasing its index");
    service.forget(root);
    forget_module_episode(root);
    let nearest = nearest_existing_dir(Path::new(root)).map(|p| slash(&p));
    sink.emit(EVT_PROJECT_MISSING, json!({ "root": slash(Path::new(root)), "nearest_existing": nearest }));
}

/// Announce `root`'s missing reactor modules, unless this exact set was already announced.
///
/// The single notification source for a broken reactor: the resolver records the same modules on
/// its [`Resolution`](bennu_maven::prelude::Resolution) for the dependency warning, and JDK detection
/// skips them without a word, so the user hears about a lost module from here and only here.
pub fn announce_missing_modules(root: &str, sink: &Arc<dyn EventSink>) {
    let report = bennu_maven::prelude::reactor_report(Path::new(root));
    let signature = signature_of(&report.missing);
    if !remember_signature(&ANNOUNCED, root, &signature) || report.missing.is_empty() {
        return;
    }
    let modules: Vec<_> = report
        .missing
        .iter()
        .map(|m| json!({ "name": m.name, "pom": slash(&m.declared_in) }))
        .collect();
    let unlisted: Vec<String> = report.unlisted.iter().map(|p| slash(p)).collect();
    sink.emit(EVT_MODULES_MISSING, json!({ "root": slash(Path::new(root)), "modules": modules, "unlisted": unlisted }));
}

/// Forget what was announced for `root`, so the next check announces afresh — a manual rebuild is
/// the user asking to be told again.
pub fn forget_module_episode(root: &str) {
    let mut guard = ANNOUNCED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(map) = guard.as_mut() {
        map.remove(root);
    }
}

fn signature_of(missing: &[bennu_maven::prelude::MissingModule]) -> String {
    let mut keys: Vec<String> = missing.iter().map(|m| format!("{}|{}", m.declared_in.display(), m.name)).collect();
    keys.sort();
    keys.join("\n")
}

/// Record `signature` for `root`; `true` when it differs from what was recorded (a first sighting
/// included).
fn remember_signature(memory: &Mutex<Option<HashMap<String, String>>>, root: &str, signature: &str) -> bool {
    let mut guard = memory.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if map.get(root).map(String::as_str) == Some(signature) {
        return false;
    }
    map.insert(root.to_string(), signature.to_string());
    true
}

// ── Handlers ─────────────────────────────────────────────────────────────────────

/// Args for [`bennu_file_owner`].
#[derive(Deserialize)]
pub struct FileOwnerArgs {
    /// Absolute path of the file on screen.
    pub file: String,
}

/// The open project owning a file.
#[derive(Debug, Clone, Serialize)]
pub struct OwnerWire {
    /// The owning project's root, forward-slashed.
    pub root: String,
    /// Whether its index build has finished.
    pub ready: bool,
}

/// Which indexed project a file belongs to, and — when none does — which directory to open.
#[derive(Debug, Clone, Serialize)]
pub struct FileOwnerWire {
    /// The owning open project, `None` when no indexed project contains the file.
    pub owner: Option<OwnerWire>,
    /// The reactor root enclosing the file, offered only when there is no owner.
    pub suggested_root: Option<String>,
}

/// Which indexed project owns `file`. A project still building IS an owner (`ready: false`): the
/// "not indexed" state is reserved for a file no index will ever cover as things stand.
#[arbor_rpc::handler]
fn bennu_file_owner(_ctx: &BennuState, args: FileOwnerArgs) -> Result<FileOwnerWire, String> {
    let owner = IndexService::global()
        .file_owner(&args.file)
        .map(|(root, ready)| OwnerWire { root, ready });
    let suggested_root = match owner {
        Some(_) => None,
        None => enclosing_maven_root(Path::new(&args.file)).map(|p| slash(&p)),
    };
    Ok(FileOwnerWire { owner, suggested_root })
}

/// Args for [`bennu_root_status`].
#[derive(Deserialize)]
pub struct RootStatusArgs {
    /// A project root as the workspace remembers it.
    pub root: String,
}

/// Whether a remembered project root is still there.
#[derive(Debug, Clone, Serialize)]
pub struct RootStatusWire {
    /// The root is an existing directory.
    pub exists: bool,
    /// When it is not: the nearest ancestor that still exists, where a picker should start.
    pub nearest_existing: Option<String>,
}

/// Tell a vanished project root apart from every other reason a project failed to open, without
/// matching the open error's prose.
#[arbor_rpc::handler]
fn bennu_root_status(_ctx: &BennuState, args: RootStatusArgs) -> Result<RootStatusWire, String> {
    let root = Path::new(&args.root);
    let exists = root.is_dir();
    let nearest_existing = if exists { None } else { nearest_existing_dir(root).map(|p| slash(&p)) };
    Ok(RootStatusWire { exists, nearest_existing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_maven::prelude::MissingModule;

    fn module(pom: &str, name: &str) -> MissingModule {
        MissingModule { name: name.to_string(), declared_in: std::path::PathBuf::from(pom) }
    }

    /// The same set is one episode however many ticks see it; a change — even to nothing — ends it.
    #[test]
    fn a_missing_module_set_is_announced_once_per_episode() {
        static M: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
        let broken = signature_of(&[module("/p/pom.xml", "web")]);
        assert!(remember_signature(&M, "/p", &broken), "first sighting");
        assert!(!remember_signature(&M, "/p", &broken), "same set, same episode");
        assert!(remember_signature(&M, "/p", ""), "fixed: the episode ends");
        assert!(remember_signature(&M, "/p", &broken), "broken again: a new episode");
    }

    /// Order is not identity: the same modules found in a different order are the same episode.
    #[test]
    fn the_signature_ignores_order() {
        let a = signature_of(&[module("/p/pom.xml", "a"), module("/p/pom.xml", "b")]);
        let b = signature_of(&[module("/p/pom.xml", "b"), module("/p/pom.xml", "a")]);
        assert_eq!(a, b);
    }
}
