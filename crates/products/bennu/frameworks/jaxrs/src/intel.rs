//! The editor's answers: the Endpoints rows, the gutter, a hover on a route, and the `@PathParam`
//! string — completed from the route's variables and navigable to the one it names.

use bennu_complete::prelude::{Proposal, Proposals};
use bennu_ext::prelude::{ExtEntry, ExtGutterMark, ExtHover, ExtTarget};
use bennu_proto::prelude::CompletionItem;

use crate::model::{AppPrefix, JaxRsModel, Lit, Param, PathSpec, ResourceMethod, Site};
use crate::paths::{simple_type, templates};

/// The rows of the shared `endpoints` catalog — shaped exactly like Spring's, so one panel lists
/// both and nobody can tell from the row which framework routed it except by its handler.
pub fn catalog(model: &JaxRsModel) -> Vec<ExtEntry> {
    let prefix = &model.ctx.prefix;
    let mut rows: Vec<ExtEntry> = model.routes().map(|m| row(m, prefix)).collect();
    rows.sort_by(|a, b| a.primary.cmp(&b.primary).then_with(|| a.kind.cmp(&b.kind)));
    rows
}

fn row(m: &ResourceMethod, prefix: &AppPrefix) -> ExtEntry {
    let mut tags = vec![simple_type(&m.return_type).to_string()];
    if !m.produces.is_empty() {
        tags.push(m.produces.clone());
    }
    if *prefix == AppPrefix::Unknown {
        tags.push("application path unknown".to_string());
    }
    ExtEntry {
        id: m.label(prefix),
        primary: m.display_path(prefix),
        secondary: m.handler(),
        kind: m.kind().to_string(),
        file: Some(m.file.clone()),
        offset: Some(m.offset),
        line: Some(m.line),
        tags,
        children: m.params.iter().map(param_row).collect(),
    }
}

fn param_row(p: &Param) -> ExtEntry {
    ExtEntry {
        id: p.name.clone(),
        primary: p.bound.as_ref().map_or_else(|| p.name.clone(), |l| l.value.clone()),
        secondary: p.type_text.clone(),
        kind: p.binding.to_string(),
        tags: if p.optional { vec!["optional".to_string()] } else { Vec::new() },
        ..ExtEntry::default()
    }
}

/// One decorative mark per route line — the route is the tooltip, which is the whole point.
pub fn gutter(buffer: &[ResourceMethod], prefix: &AppPrefix) -> Vec<ExtGutterMark> {
    let mut out: Vec<ExtGutterMark> = Vec::new();
    for m in buffer.iter().filter(|m| m.listed) {
        if out.iter().any(|g| g.line == m.line) {
            continue;
        }
        out.push(ExtGutterMark {
            line: m.line,
            kind: "endpoint".to_string(),
            tooltip: m.label(prefix),
            targets: Vec::new(),
        });
    }
    out
}

/// The route a verb or method-level `@Path` annotation contributes to.
pub fn hover(buffer: &[ResourceMethod], prefix: &AppPrefix, offset: usize) -> Option<ExtHover> {
    let covers = |span: Option<(usize, usize)>| span.is_some_and(|(s, e)| offset >= s && offset <= e);
    let m = buffer.iter().find(|m| covers(m.verb_span) || covers(m.path_span))?;
    let mut doc = Vec::new();
    if !m.produces.is_empty() {
        doc.push(format!("Produces: {}", m.produces));
    }
    if !m.consumes.is_empty() {
        doc.push(format!("Consumes: {}", m.consumes));
    }
    if !m.class_path.is_written() {
        doc.push("The class has no class-level @Path: this is reached only through a sub-resource locator.".to_string());
    }
    if *prefix == AppPrefix::Unknown {
        doc.push("The application path is declared more than once, or not as a literal, so it is left out.".to_string());
    }
    Some(ExtHover { title: m.label(prefix), signature: m.handler(), doc: doc.join("\n") })
}

/// Inside `@PathParam("…")`: the variables of that method's route, in the order they appear.
pub fn completions(
    buffer: &[ResourceMethod],
    prefix: &AppPrefix,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let Some((m, lit)) = path_param_at(buffer, offset) else { return Vec::new() };
    let typed = source.get(lit.start..offset).unwrap_or("");
    // Every readable piece, even when another is not: a completion list may offer what it knows,
    // where a diagnostic may not claim what it does not.
    let mut names: Vec<String> = Vec::new();
    for piece in [prefix.text(), m.class_path.text().unwrap_or(""), m.method_path.text().unwrap_or("")] {
        for t in templates(piece).unwrap_or_default() {
            if !names.contains(&t.name) {
                names.push(t.name);
            }
        }
    }
    let route = m.label(prefix);
    let mut out = Proposals::default();
    for name in names.into_iter().filter(|n| n.starts_with(typed)) {
        out.offer(Proposal::new(name, "path-variable").detail(route.clone()));
    }
    out.into_items()
        .into_iter()
        .map(|item| CompletionItem { replace_start: Some(lit.start), replace_end: Some(lit.end), ..item })
        .collect()
}

/// From `@PathParam("id")` to the `{id}` it binds — in the method's `@Path`, then the class's.
pub fn navigate(buffer: &[ResourceMethod], offset: usize) -> Vec<ExtTarget> {
    let Some((m, lit)) = path_param_at(buffer, offset) else { return Vec::new() };
    let mut out = Vec::new();
    for spec in [&m.method_path, &m.class_path] {
        let PathSpec::Literal(path) = spec else { continue };
        for t in templates(&path.value).unwrap_or_default() {
            if t.name != lit.value {
                continue;
            }
            out.push(ExtTarget {
                file: path.file.clone(),
                offset: path.start + t.name_start,
                label: format!("{{{}}}", t.name),
                detail: format!("@Path(\"{}\") · {}", path.value, m.handler()),
            });
        }
    }
    out
}

/// The declared resource method whose `@PathParam` literal holds the caret.
fn path_param_at(buffer: &[ResourceMethod], offset: usize) -> Option<(&ResourceMethod, &Lit)> {
    buffer.iter().filter(|m| m.site == Site::Declared).find_map(|m| {
        m.params
            .iter()
            .filter(|p| p.binding == "path")
            .filter_map(|p| p.bound.as_ref())
            .find(|l| l.file == m.file && offset >= l.start && offset <= l.end)
            .map(|l| (m, l))
    })
}
