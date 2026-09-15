//! The model as catalog rows — the shape every framework list panel in bennu already renders.
//!
//! Three catalogs, and the split between them is the split between three questions:
//!
//! * **components** — *what data does this game have, and who touches it?* The row is the type; the
//!   sub-rows are the systems that read it, the ones that write it, and the bundles that carry it.
//!   This is find-usages asked properly: "who touches `Health`" is a question about signatures, and
//!   a text search answers it with every comment that mentions the word.
//! * **systems** — *what runs, in which schedule, over what?* Badged by schedule, so grouping by
//!   badge groups by schedule.
//! * **conflicts** — *which pairs can never run at the same time, and why?* See
//!   [`crate::conflict`] for what that claim is worth under a partial scan.
//! * **shaders** — *which `.wgsl` does each material run, and do the two agree?* Its own catalog
//!   rather than a section of the components one, because the row is a **file** and the sub-rows
//!   are the materials that name it — the opposite direction from every other list here, and
//!   frequently one-to-many. It is the same pair of panels a Struts project gets: one keyed on
//!   the declaration, one keyed on the thing it points at.

use bennu_ext::prelude::ExtEntry;

use crate::conflict::Conflict;
use crate::model::{access_keys, Access, BevyModel, InsertSite, Role, SystemDecl, TypeDecl};
use crate::shader_link::{Severity, ShaderLink};

/// Absolute, forward-slashed — the form every contributed site uses, so the frontend never has to
/// care which separator the host prefers.
fn file_of(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn row(primary: String, secondary: String, kind: String) -> ExtEntry {
    ExtEntry {
        id: primary.clone(),
        primary,
        secondary,
        kind,
        file: None,
        offset: None,
        line: None,
        tags: Vec::new(),
        children: Vec::new(),
    }
}

/// Attach a source site to a row.
fn at(mut entry: ExtEntry, path: &std::path::Path, offset: usize, line: u32) -> ExtEntry {
    entry.file = Some(file_of(path));
    entry.offset = Some(offset);
    entry.line = Some(line);
    entry
}

/// One row per declared ECS type, with what touches it underneath.
pub fn components(model: &BevyModel) -> Vec<ExtEntry> {
    model.types.iter().map(|t| component_row(model, t)).collect()
}

fn component_row(model: &BevyModel, t: &TypeDecl) -> ExtEntry {
    let touching = model.touching(&access_keys(&t.name, &t.roles));
    let writers = touching.iter().filter(|(_, a)| a.kind.writes()).count();
    let readers = touching.len() - writers;
    let filtering = model.filtering(&t.name);
    let bundles = model.bundles_with(&t.name);

    let mut children: Vec<ExtEntry> = touching
        .iter()
        .map(|(s, a)| system_child(s, a))
        .chain(filtering.iter().map(|(s, a)| {
            let mut e = system_child(s, a);
            e.kind = "filter".to_string();
            e.id = format!("{}#filter#{}", s.name, t.name);
            e
        }))
        .chain(bundles.iter().map(|b| {
            at(
                row(b.name.clone(), format!("carries {}", t.name), "in bundle".to_string()),
                &b.file,
                b.offset,
                b.line,
            )
        }))
        .collect();
    // Where it is put into the world. First, above everything that merely reads it: a type that
    // six systems read and nothing ever creates is a type whose row was previously all
    // consumers and no producer, and the producer is what you actually go looking for.
    for site in model.inserted(&t.name) {
        children.insert(0, insert_child(site));
    }

    // A material is an asset that also runs a shader, and the shader is the first thing anybody
    // reading its row wants. Badged rather than given a role of its own: `Material` is a trait
    // impl on top of `#[derive(Asset)]`, and an enum of ECS roles is not where a trait impl goes.
    if let Some(material) = model.materials.iter().find(|m| m.name == t.name) {
        // The bind group, above the shaders: what the material SUPPLIES, then what runs on it.
        // A binding row also says what the shader declares at that index, which is the whole
        // point of having read both files — `@binding(0) var<uniform> params: SpiralHoverParams`
        // beside `#[uniform(0)] params: SpiralHoverParams` is the agreement, visible.
        for (n, b) in material.bindings.iter().enumerate().rev() {
            let declared = model
                .shaders
                .iter()
                .filter(|l| material.shaders.iter().any(|s| s.path == l.asset_path))
                .find_map(|l| l.wgsl_binding(b.index));
            let secondary = match declared {
                Some(d) => format!("{} · shader: {d}", b.ty),
                None => b.ty.clone(),
            };
            let mut child = at(
                row(
                    format!("@binding({}) {}", b.index, b.field),
                    secondary,
                    b.kind.label().to_string(),
                ),
                &material.file,
                b.offset,
                b.line,
            );
            child.id = format!("{}#binding#{n}", t.name);
            children.insert(0, child);
        }
        for used in &material.shaders {
            let resolved = model.shader(&used.path).and_then(|l| l.file.clone());
            let mut child = row(
                used.path.clone(),
                match resolved {
                    Some(_) => format!("{} shader", used.stage),
                    None => format!("{} shader — no such asset", used.stage),
                },
                "shader".to_string(),
            );
            child.id = format!("{}#shader#{}", t.name, used.stage);
            if let Some(file) = resolved {
                child.file = Some(file_of(&file));
                child.offset = Some(0);
                child.line = Some(1);
            }
            children.insert(0, child);
        }
    }
    // Fields first for a bundle: what it inserts is what the row is *about*, and it is the only
    // catalog row whose own declaration lists other rows.
    if t.roles.contains(&Role::Bundle) {
        let fields: Vec<ExtEntry> = t
            .fields
            .iter()
            .map(|f| crate::params::type_key(f))
            .map(|name| match model.types.iter().find(|d| d.name == name) {
                Some(d) => at(
                    row(name.clone(), primary_role(d), "inserts".to_string()),
                    &d.file,
                    d.offset,
                    d.line,
                ),
                None => row(name, "declared outside this project".into(), "inserts".into()),
            })
            .collect();
        children.splice(0..0, fields);
    }

    let mut entry = at(
        row(t.name.clone(), touch_summary(readers, writers, filtering.len()), primary_role(t)),
        &t.file,
        t.offset,
        t.line,
    );
    // Every role beyond the badge, so a type that is both a Component and an Event says so.
    entry.tags = t.roles.iter().skip(1).map(|r| r.label().to_string()).collect();
    if writers == 0 && readers > 0 {
        entry.tags.push("read-only".to_string());
    }
    entry.children = children;
    entry
}

fn primary_role(t: &TypeDecl) -> String {
    t.roles.first().map_or_else(|| "type".to_string(), |r| r.label().to_string())
}

fn touch_summary(readers: usize, writers: usize, filters: usize) -> String {
    let mut parts = Vec::new();
    if readers > 0 {
        parts.push(format!("read by {readers}"));
    }
    if writers > 0 {
        parts.push(format!("written by {writers}"));
    }
    // Named apart from the two above rather than added to them: a filter is not an access, and a
    // marker read by nobody and filtered on by twelve systems should say exactly that.
    if filters > 0 {
        parts.push(format!("filtered on by {filters}"));
    }
    if parts.is_empty() {
        return "no system in this project reads, writes or filters on it".to_string();
    }
    parts.join(" · ")
}

fn system_child(s: &SystemDecl, a: &Access) -> ExtEntry {
    let schedule = s.schedules.first().cloned().unwrap_or_else(|| "unregistered".to_string());
    let mut e = at(
        row(s.name.clone(), a.param.clone(), a.kind.label().to_string()),
        &s.file,
        s.offset,
        s.line,
    );
    e.id = format!("{}#{}", s.name, a.target);
    e.tags = vec![schedule];
    e
}

/// One row per system, badged by the schedule it was registered in.
pub fn systems(model: &BevyModel) -> Vec<ExtEntry> {
    model
        .systems
        .iter()
        .map(|s| {
            let schedule =
                s.schedules.first().cloned().unwrap_or_else(|| "unregistered".to_string());
            let mut e = at(
                row(s.name.clone(), s.access_summary(), schedule),
                &s.file,
                s.offset,
                s.line,
            );
            e.tags = s.schedules.iter().skip(1).cloned().chain(s.sets.iter().cloned()).collect();
            if s.exclusive {
                e.tags.push("exclusive".to_string());
            }
            if s.schedules.is_empty() {
                // Not a defect: a system registered by a helper this scan cannot follow looks
                // exactly like one nobody registered, and saying which it is would be a guess.
                e.tags.push("no add_systems call found here".to_string());
            }
            e.children = s
                .accesses
                .iter()
                .map(|a| {
                    let mut c = row(a.target.clone(), a.param.clone(), a.kind.label().to_string());
                    c.id = format!("{}#{}#{}", s.name, a.target, a.kind.label());
                    c.tags = a
                        .filters
                        .iter()
                        .map(|f| match f {
                            crate::model::Filter::With(t) => format!("With<{t}>"),
                            crate::model::Filter::Without(t) => format!("Without<{t}>"),
                        })
                        .collect();
                    c
                })
                .collect();
            e
        })
        .collect()
}

/// One row per pair the engine must serialise.
pub fn conflicts(model: &BevyModel) -> Vec<ExtEntry> {
    model.conflicts.iter().map(|c| conflict_row(model, c)).collect()
}

fn conflict_row(model: &BevyModel, c: &Conflict) -> ExtEntry {
    let (a, b) = (&model.systems[c.a], &model.systems[c.b]);
    let targets: Vec<String> = c.reasons.iter().map(|r| r.target.clone()).collect();
    let mut entry = at(
        row(
            format!("{} ⇄ {}", a.name, b.name),
            format!("contend over {}", targets.join(", ")),
            c.schedule.clone(),
        ),
        &a.file,
        a.offset,
        a.line,
    );
    entry.id = format!("{}|{}|{}", a.name, b.name, c.schedule);
    entry.tags = vec![c.ordering.label().to_string()];
    entry.children = c
        .reasons
        .iter()
        .flat_map(|r| {
            let head = {
                let mut h = row(
                    r.target.clone(),
                    format!("{} — {}", r.kind, r.a_param),
                    "contended".to_string(),
                );
                h.id = format!("{}|{}|{}", a.name, b.name, r.target);
                h
            };
            // The other side gets its own row rather than being squeezed into the same line: each
            // half names a system and a parameter, and each half is a place to jump to.
            let other = at(
                row(b.name.clone(), r.b_param.clone(), "and".to_string()),
                &b.file,
                b.offset,
                b.line,
            );
            [at(head, &a.file, a.offset, a.line), other]
        })
        .collect();
    entry
}

/// One row per shader a material names, with the materials under it and whatever the two
/// disagree about.
///
/// A shader with problems sorts to the top: the list is read to find out whether anything is
/// wrong, and a file with a layout mismatch buried under nine correct ones is a file nobody
/// sees.
pub fn shaders(model: &BevyModel) -> Vec<ExtEntry> {
    let mut rows: Vec<ExtEntry> = model.shaders.iter().map(|l| shader_row(l)).collect();
    rows.sort_by_key(|r| (r.tags.is_empty(), r.primary.clone()));
    rows
}

fn shader_row(link: &ShaderLink) -> ExtEntry {
    let name = link.asset_path.rsplit('/').next().unwrap_or(&link.asset_path).to_string();
    let errors = link.problems.iter().filter(|p| p.severity == Severity::Error).count();
    let warnings = link.problems.len() - errors;

    let secondary = match link.file {
        // Not resolved. Said first and said plainly: every other row under it is about a file
        // that is not there.
        None => format!("{} — no such asset", link.asset_path),
        Some(_) => link.asset_path.clone(),
    };
    // "No material here" rather than "0 material(s)": on a game whose materials live in the
    // engine crate it depends on, that is every row, and a column of zeroes reads as a fault.
    let kind = match link.uses.len() {
        0 => "no material in this project".to_string(),
        n => format!("{n} material(s)"),
    };
    let mut entry = row(name, secondary, kind);
    entry.id = format!("shader:{}", link.asset_path);
    if let Some(file) = &link.file {
        entry.file = Some(file_of(file));
        entry.offset = Some(0);
        entry.line = Some(1);
    }
    if errors > 0 {
        entry.tags.push(format!("{errors} error(s)"));
    }
    if warnings > 0 {
        entry.tags.push(format!("{warnings} warning(s)"));
    }

    // The materials that name it, then what is wrong. In that order because the first answers
    // "whose is this" and the second only makes sense once you know.
    for used in &link.uses {
        let mut child = at(
            row(
                used.material.clone(),
                format!("{} stage", used.stage),
                "material".to_string(),
            ),
            &used.file,
            used.offset,
            used.line,
        );
        child.id = format!("{}#{}#{}", link.asset_path, used.material, used.stage);
        entry.children.push(child);
    }
    for (n, problem) in link.problems.iter().enumerate() {
        let mut child = at(
            row(problem.message.clone(), problem.code.clone(), problem.severity.as_str().to_string()),
            &problem.file,
            problem.start,
            // The line is not carried on a problem — the panel jumps by offset, and the editor
            // resolves the line from it. `0` rather than a wrong number.
            0,
        );
        child.id = format!("{}#problem#{n}", link.asset_path);
        entry.children.push(child);
    }
    entry
}

/// One site that puts a declaration into the world.
fn insert_child(site: &InsertSite) -> ExtEntry {
    let primary = match site.in_fn.is_empty() {
        true => site.file.file_name().map_or_else(
            || site.type_name.clone(),
            |n| n.to_string_lossy().into_owned(),
        ),
        false => site.in_fn.clone(),
    };
    let mut e = at(
        row(primary, site.arg.clone(), site.kind.label().to_string()),
        &site.file,
        site.offset,
        site.line,
    );
    e.id = format!("{}#at#{}#{}", site.type_name, site.file.to_string_lossy(), site.offset);
    e
}
