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
use crate::model::{
    BevyModel, BindingDecl, InsertSite, MaterialDecl, Role, ShaderRefDecl, SystemDecl, TypeDecl,
    UniformField, UniformStruct,
};
use crate::shader_link::{asset_paths_of, ShaderFile};
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
///
/// `shaders` is every `.wgsl` in the project. It is the second half of the material check: a
/// `#[derive(AsBindGroup)]` and a `struct` in a shader describe the same block of bytes, and
/// nothing in the toolchain verifies that they agree — see [`crate::shader_link`].
pub fn build(sources: &[ScannedFile], shaders: &[ScannedFile]) -> BevyModel {
    // Masked once and kept: the ECS scan and the material scan both read it, and masking is the
    // most expensive thing either of them does.
    let masked: Vec<String> = sources.iter().map(|f| mask(&f.text)).collect();
    let scans: Vec<(&ScannedFile, FileScan)> =
        sources.iter().zip(&masked).map(|(f, m)| (f, items::scan_file(m))).collect();

    let mut model = BevyModel::default();
    collect_types(&scans, &mut model);
    collect_materials(sources, &masked, &mut model);

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
    collect_inserts(&scans, &mut model);

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

    // Last, because it needs both halves: the materials collected above and the shaders the host
    // walked. A shader outside an `assets/` directory is skipped rather than matched by name —
    // see `asset_path_of`.
    let files: Vec<ShaderFile<'_>> = shaders
        .iter()
        .map(|f| ShaderFile {
            path: &f.path,
            text: &f.text,
            asset_paths: asset_paths_of(&f.path),
        })
        .filter(|f| !f.asset_paths.is_empty())
        .collect();
    model.shaders = crate::shader_link::link(&model.materials, &model.uniforms, &files);
    model
}

/// Every site that puts a declaration into the world, with the function it happens in.
///
/// The enclosing function is the last one declared above the site. Approximate, and deliberately
/// so: computing it exactly would mean tracking brace depth through a file this crate reads by
/// shape, and the cost of being wrong is a label — the jump is to the call, which is always
/// right.
fn collect_inserts(scans: &[(&ScannedFile, FileScan)], model: &mut BevyModel) {
    for (file, scan) in scans {
        // Sorted once per file rather than searched linearly per site: a file with two hundred
        // spawns in it is a plugin, and those exist.
        let mut fns: Vec<(usize, &str)> =
            scan.fns.iter().map(|f| (f.offset, f.name.as_str())).collect();
        fns.sort_unstable();
        for raw in &scan.inserts {
            let in_fn = match fns.partition_point(|(at, _)| *at <= raw.offset) {
                0 => String::new(),
                n => fns[n - 1].1.to_string(),
            };
            model.inserts.push(InsertSite {
                type_name: raw.type_name.clone(),
                kind: raw.kind,
                file: file.path.clone(),
                offset: raw.offset,
                line: line_number(&file.text, raw.offset),
                arg: raw.arg.clone(),
                in_fn,
            });
        }
    }
    model.inserts.sort_by(|a, b| (&a.type_name, &a.file, a.offset).cmp(&(&b.type_name, &b.file, b.offset)));
}

/// The `#[derive(AsBindGroup)]` materials, their bindings, and the `ShaderType` layouts.
fn collect_materials(sources: &[ScannedFile], masked: &[String], model: &mut BevyModel) {
    for (file, m) in sources.iter().zip(masked) {
        let scan = crate::shader::scan(m, &file.text);
        // A material's shader refs come from its `impl Material for …`, which is very often in
        // the same file and occasionally is not. Joined by type name, like everything else here.
        for raw in scan.materials {
            let shaders: Vec<ShaderRefDecl> = scan
                .refs
                .iter()
                .filter(|r| r.type_name == raw.name)
                .map(|r| ShaderRefDecl {
                    stage: r.stage.clone(),
                    path: r.path.clone(),
                    offset: r.offset,
                    end: r.end,
                    line: line_number(&file.text, r.offset),
                })
                .collect();
            model.materials.push(MaterialDecl {
                name: raw.name,
                file: file.path.clone(),
                offset: raw.offset,
                line: line_number(&file.text, raw.offset),
                bindings: raw
                    .bindings
                    .into_iter()
                    .map(|b| BindingDecl {
                        index: b.index,
                        kind: b.kind,
                        field: b.field,
                        ty: b.ty,
                        offset: b.offset,
                        line: line_number(&file.text, b.offset),
                    })
                    .collect(),
                shaders,
            });
        }
        for raw in scan.structs {
            model.uniforms.push(UniformStruct {
                name: raw.name,
                file: file.path.clone(),
                offset: raw.offset,
                line: line_number(&file.text, raw.offset),
                fields: raw
                    .fields
                    .into_iter()
                    .map(|f| UniformField {
                        name: f.name,
                        ty: f.ty,
                        offset: f.offset,
                        line: line_number(&file.text, f.offset),
                    })
                    .collect(),
            });
        }
    }
    // A material declared in one file and `impl`-ed in another: fold the refs in afterwards.
    let orphans: Vec<(String, ShaderRefDecl)> = sources
        .iter()
        .zip(masked)
        .flat_map(|(file, m)| {
            let scan = crate::shader::scan(m, &file.text);
            scan.refs
                .into_iter()
                .map(|r| {
                    let line = line_number(&file.text, r.offset);
                    (
                        r.type_name,
                        ShaderRefDecl {
                            stage: r.stage,
                            path: r.path,
                            offset: r.offset,
                            end: r.end,
                            line,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for (type_name, r) in orphans {
        if let Some(m) = model.materials.iter_mut().find(|m| m.name == type_name) {
            if !m.shaders.iter().any(|s| s.stage == r.stage && s.path == r.path) {
                m.shaders.push(r);
            }
        }
    }
    model.materials.sort_by(|a, b| a.name.cmp(&b.name));
    model.uniforms.sort_by(|a, b| a.name.cmp(&b.name));
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

#[cfg(test)]
mod insert_tests {
    use std::path::PathBuf;

    use bennu_ext::prelude::ScannedFile;

    use crate::items::InsertKind;

    use super::build;

    const GAME: &str = r#"
use bevy::prelude::*;

#[derive(Component)] pub struct Health(pub f32);
#[derive(Component)] pub struct Player;
#[derive(Resource, Default)] pub struct Score { points: u32 }
#[derive(Message)] pub struct Damaged;
#[derive(States, Default, Hash, Eq, PartialEq, Clone, Debug)] pub enum Phase { #[default] Menu }

fn spawn_player(mut commands: Commands) {
    commands.spawn((Health(100.0), Player, Transform::from_xyz(0.0, 0.0, 0.0)));
}

fn wound(mut commands: Commands, e: Entity) {
    commands.entity(e).insert(Health(10.0));
    // commands.spawn(Ghost) — a comment, not a site.
    let bag = HashMap::new();
    bag.insert(key, value);
}

pub fn plugin(app: &mut App) {
    app.insert_resource(Score::default());
    app.init_resource::<Score>();
    app.add_message::<Damaged>();
    app.init_state::<Phase>();
}
"#;

    fn model() -> crate::model::BevyModel {
        build(
            &[ScannedFile { path: PathBuf::from("/p/src/game.rs"), text: GAME.to_string() }],
            &[],
        )
    }

    fn kinds(name: &str) -> Vec<InsertKind> {
        model().inserted(name).iter().map(|s| s.kind).collect()
    }

    #[test]
    fn a_spawned_tuple_is_one_site_per_component() {
        let m = model();
        for name in ["Health", "Player", "Transform"] {
            assert!(
                m.inserted(name).iter().any(|s| s.kind == InsertKind::Spawn),
                "`{name}` is spawned in the tuple"
            );
        }
    }

    #[test]
    fn a_site_says_which_function_it_is_in_and_what_it_was_given() {
        let m = model();
        let site = m
            .inserted("Health")
            .into_iter()
            .find(|s| s.kind == InsertKind::Spawn)
            .expect("spawned");
        assert_eq!(site.in_fn, "spawn_player");
        assert_eq!(site.arg, "Health(100.0)");
        // The jump is to the CALL, so it lands on the statement rather than inside an expression
        // that may not be a token at all.
        assert!(GAME[site.offset..].starts_with("spawn"));
    }

    #[test]
    fn an_insert_on_an_existing_entity_is_told_apart_from_a_spawn() {
        assert!(kinds("Health").contains(&InsertKind::Insert));
        let m = model();
        let added =
            m.inserted("Health").into_iter().find(|s| s.kind == InsertKind::Insert).unwrap();
        assert_eq!(added.in_fn, "wound");
    }

    #[test]
    fn resources_messages_and_states_each_get_their_own_kind() {
        assert!(kinds("Score").contains(&InsertKind::Resource));
        assert!(kinds("Damaged").contains(&InsertKind::Register));
        assert!(kinds("Phase").contains(&InsertKind::State));
    }

    #[test]
    fn both_the_turbofish_and_the_argument_form_are_read() {
        // `insert_resource(Score::default())` and `init_resource::<Score>()` are the same fact
        // written two ways, and a project uses both.
        assert_eq!(kinds("Score").len(), 2);
    }

    #[test]
    fn a_commented_out_spawn_is_not_a_site() {
        assert!(model().inserted("Ghost").is_empty());
    }

    #[test]
    fn a_map_insert_contributes_nothing() {
        // `bag.insert(key, value)` is the shape this must not turn into a row. Both arguments
        // are lower-case bindings, and a value whose type this cannot name is skipped rather
        // than guessed at.
        let m = model();
        assert!(m.inserts.iter().all(|s| s.type_name != "key" && s.type_name != "value"));
    }
}
