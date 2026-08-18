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

use bennu_ext::prelude::ExtEntry;

use crate::conflict::Conflict;
use crate::model::{access_keys, Access, BevyModel, Role, SystemDecl, TypeDecl};

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
