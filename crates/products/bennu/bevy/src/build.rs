//! Assembling one model out of every file the host handed over.
//!
//! Three things can only be decided once the whole project has been read, which is why they are
//! here rather than in [`crate::items`]:
//!
//! * **Which functions are systems.** A function is one if it takes a system parameter — and a
//!   project's own `#[derive(SystemParam)]` struct is a system parameter, defined in whichever file
//!   its author chose.
//! * **What each parameter accesses.** Same reason: expanding a bundled parameter needs its
//!   declaration.
//! * **Which schedule a system is in.** The `add_systems` call is rarely in the file that declares
//!   the system, and is very often in a third one — the plugin.

use std::collections::HashMap;

use bennu_complete::prelude::line_number;
use bennu_ext::prelude::ScannedFile;

use crate::conflict::{self, OrderIndex};
use crate::items::{self, FileScan};
use crate::mask::mask;
use crate::model::{BevyModel, Role, SystemDecl, TypeDecl};
use crate::params::{accesses_for, head_and_args, type_of_binding};

/// Parameter heads that make a function a system on their own — the ones Bevy's own prelude
/// provides. A project `SystemParam` joins them once its declaration has been read.
const SYSTEM_PARAMS: &[&str] = &[
    "Query",
    "Populated",
    "Single",
    "Res",
    "ResMut",
    "NonSend",
    "NonSendMut",
    "Commands",
    "ParallelCommands",
    "EventReader",
    "EventWriter",
    "MessageReader",
    "MessageWriter",
    "EventMutator",
    "MessageMutator",
    "ParamSet",
    "Local",
    "Gizmos",
    "Deferred",
    "World",
    // An observer's trigger. Not registered with `add_systems`, so this is the only thing that
    // makes an observer visible at all.
    "On",
    "Trigger",
];

/// Read every `.rs` the host walked, and answer as one model.
pub fn build(sources: &[ScannedFile]) -> BevyModel {
    let scans: Vec<(&ScannedFile, FileScan)> =
        sources.iter().map(|f| (f, items::scan_file(&mask(&f.text)))).collect();

    let mut model = BevyModel::default();
    collect_types(&scans, &mut model);

    // The two lookups the second pass needs: what a bundled parameter expands to, and which type
    // names are system parameters at all.
    let custom: HashMap<String, Vec<String>> = model
        .types
        .iter()
        .filter(|t| t.roles.contains(&Role::SystemParam))
        .map(|t| (t.name.clone(), t.fields.clone()))
        .collect();

    let registrations = index_registrations(&scans);
    collect_systems(&scans, &custom, &registrations, &mut model);

    let mut order = OrderIndex::default();
    for (_, scan) in &scans {
        for reg in &scan.registrations {
            for name in &reg.systems {
                for other in &reg.ordered_with {
                    order.add(name, other);
                }
                if reg.chained {
                    for peer in &reg.systems {
                        order.add(name, peer);
                    }
                }
            }
        }
    }
    model.conflicts = conflict::detect(&model.systems, &order);
    model
}

/// What one `add_systems` call said about a system: its schedule, and the sets it was put in.
#[derive(Default)]
struct Registered {
    schedules: Vec<String>,
    sets: Vec<String>,
}

fn index_registrations(scans: &[(&ScannedFile, FileScan)]) -> HashMap<String, Registered> {
    let mut out: HashMap<String, Registered> = HashMap::new();
    for (_, scan) in scans {
        for reg in &scan.registrations {
            for name in &reg.systems {
                let entry = out.entry(name.clone()).or_default();
                if !entry.schedules.contains(&reg.schedule) {
                    entry.schedules.push(reg.schedule.clone());
                }
                for set in &reg.sets {
                    if !entry.sets.contains(set) {
                        entry.sets.push(set.clone());
                    }
                }
            }
        }
    }
    out
}

/// Declarations, one row per site — and a hand-written `impl Component for Health` adds its role to
/// every declaration of that name, which is the simple-name approximation this crate is built on
/// showing through (see [`crate::model`]).
fn collect_types(scans: &[(&ScannedFile, FileScan)], model: &mut BevyModel) {
    for (file, scan) in scans {
        for raw in &scan.types {
            model.types.push(TypeDecl {
                name: raw.name.clone(),
                roles: raw.roles.clone(),
                file: file.path.clone(),
                offset: raw.offset,
                line: line_number(&file.text, raw.offset),
                fields: raw.fields.clone(),
            });
        }
    }
    // A hand-written impl adds a role to a type that was declared with none of its own — and to
    // one that was never declared here at all, which is a `Component` on a foreign type and not
    // something this crate can show a location for.
    for (_, scan) in scans {
        for (name, role) in &scan.trait_impls {
            if let Some(t) = model.types.iter_mut().find(|t| &t.name == name) {
                if !t.roles.contains(role) {
                    t.roles.push(*role);
                    t.roles.sort();
                }
            }
        }
    }
    model.types.sort_by(|a, b| a.name.cmp(&b.name));
}

fn collect_systems(
    scans: &[(&ScannedFile, FileScan)],
    custom: &HashMap<String, Vec<String>>,
    registrations: &HashMap<String, Registered>,
    model: &mut BevyModel,
) {
    for (file, scan) in scans {
        for raw in &scan.fns {
            let registered = registrations.get(&raw.name);
            if registered.is_none() && !looks_like_system(&raw.params, custom) {
                continue;
            }
            let (accesses, exclusive) = accesses_for(&raw.params, custom);
            model.systems.push(SystemDecl {
                name: raw.name.clone(),
                file: file.path.clone(),
                offset: raw.offset,
                line: line_number(&file.text, raw.offset),
                accesses,
                exclusive,
                schedules: registered.map(|r| r.schedules.clone()).unwrap_or_default(),
                sets: registered.map(|r| r.sets.clone()).unwrap_or_default(),
            });
        }
    }
    model.systems.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Whether a parameter list makes a function a system.
///
/// Shape, not registration: a system that is only ever added by a helper this scan cannot follow is
/// still a system, and a project halfway through writing one has not registered it yet. The cost is
/// the odd plain function that happens to take a `Local` — visible in the catalog with no schedule,
/// which is the honest way to show it.
fn looks_like_system(params: &[String], custom: &HashMap<String, Vec<String>>) -> bool {
    params.iter().any(|p| {
        let head = head_and_args(type_of_binding(p)).0;
        SYSTEM_PARAMS.contains(&head.as_str())
            || crate::wrappers::lookup(&head).is_some()
            || custom.contains_key(&head)
    })
}
