//! Reading resource methods out of a parsed file, interface inheritance included.
//!
//! ## The inheritance rule, and why it is read exactly
//!
//! JAX-RS lets the annotations live on an interface and the code on the class that implements it —
//! a shape legacy projects use to share one contract between a server and its clients. The runtime
//! reads the interface's annotations for an implementation method **only when that method carries
//! no JAX-RS annotation of its own**; one `@Produces` on the implementation and every annotation on
//! the interface method is ignored.
//!
//! So a pair is resolved only when it is unambiguous: the interface is found in the project, the
//! class names it through its imports, no other class implements it, and the method matches by
//! name and arity exactly once. Anything short of that and the interface's routes are reported
//! where they are written, marked unresolved — listed, never judged.

use std::collections::HashMap;

use bennu_facts::prelude::{AnnFacts, AnnString, MethodFacts, TypeFacts};

use crate::known::{self, CustomVerb};
use crate::model::{Implementor, Lit, Param, PathSpec, ResourceMethod, Site, Src};
use crate::paths::line_at;

/// The project-wide facts an extraction reads against.
pub struct Env<'a> {
    /// Files declaring an annotated interface — the ones a class may inherit routes from.
    pub interfaces: Vec<Src<'a>>,
    pub implementors: &'a HashMap<String, Vec<Implementor>>,
    pub custom_verbs: &'a [CustomVerb],
}

/// Every resource method and sub-resource locator a file declares or serves.
pub fn extract(unit: Src<'_>, env: &Env<'_>) -> Vec<ResourceMethod> {
    let mut out = Vec::new();
    for t in &unit.facts.types {
        match t.kind {
            "interface" => interface_methods(unit, t, env, &mut out),
            "class" | "record" => class_methods(unit, t, env, &mut out),
            _ => {}
        }
    }
    out
}

/// Whether an interface carries anything JAX-RS routes: a class-level `@Path`, or a method with a
/// verb or a `@Path`.
pub fn is_resource_interface(unit: Src<'_>, t: &TypeFacts, custom: &[CustomVerb]) -> bool {
    t.kind == "interface"
        && (known::find(&t.annotations, unit.facts, "Path").is_some()
            || t.methods.iter().any(|m| is_resource_method(m, unit, custom)))
}

fn is_resource_method(m: &MethodFacts, unit: Src<'_>, custom: &[CustomVerb]) -> bool {
    !m.is_constructor
        && (known::verb_of(m, unit.facts, custom).is_some()
            || known::find(&m.annotations, unit.facts, "Path").is_some())
}

/// Every project class implementing an annotated interface, keyed by the interface's FQCN.
pub fn implementors(
    units: &[Src<'_>],
    interfaces: &[Src<'_>],
    custom: &[CustomVerb],
) -> HashMap<String, Vec<Implementor>> {
    let mut out: HashMap<String, Vec<Implementor>> = HashMap::new();
    for u in units {
        for t in u.facts.types.iter().filter(|t| matches!(t.kind, "class" | "record")) {
            for written in &t.implements {
                let Some((_, it)) = resolve_interface(written, *u, interfaces, custom) else { continue };
                out.entry(it.fqcn.clone()).or_default().push(implementor(*u, t, custom));
            }
        }
    }
    out
}

fn implementor(u: Src<'_>, t: &TypeFacts, custom: &[CustomVerb]) -> Implementor {
    let sig = |m: &MethodFacts| (m.name.clone(), m.params.len());
    let methods = t.methods.iter().filter(|m| !m.is_constructor);
    Implementor {
        fqcn: t.fqcn.clone(),
        has_class_path: known::find(&t.annotations, u.facts, "Path").is_some(),
        methods: methods.clone().map(sig).collect(),
        reannotated: methods.filter(|m| known::has_method_annotations(m, u.facts, custom)).map(sig).collect(),
    }
}

/// The one annotated interface `written` names from `unit`, if exactly one does.
fn resolve_interface<'e>(
    written: &str,
    unit: Src<'_>,
    interfaces: &[Src<'e>],
    custom: &[CustomVerb],
) -> Option<(Src<'e>, &'e TypeFacts)> {
    let base = written.split('<').next().unwrap_or(written).trim();
    let mut hits = Vec::new();
    for iu in interfaces {
        let iu: Src<'e> = *iu;
        for it in &iu.facts.types {
            if known::refers_to(base, unit.facts, &it.fqcn) && is_resource_interface(iu, it, custom) {
                hits.push((iu, it));
            }
        }
    }
    match hits.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

fn interface_methods(unit: Src<'_>, t: &TypeFacts, env: &Env<'_>, out: &mut Vec<ResourceMethod>) {
    if !is_resource_interface(unit, t, env.custom_verbs) {
        return;
    }
    let impls: &[Implementor] = match env.implementors.get(&t.fqcn) {
        Some(v) => v,
        None => &[],
    };
    let class = class_level(unit, t, None);
    let rooted = class.path.is_written();
    for m in t.methods.iter().filter(|m| is_resource_method(m, unit, env.custom_verbs)) {
        let sig = (m.name.clone(), m.params.len());
        let sides = Sides { site: unit, site_type: t, site_method: m, source: unit, source_method: m };
        let mut rm = build(&sides, &class, env.custom_verbs, Site::Declared);
        (rm.listed, rm.resolved) = match impls {
            // Nothing in the project implements it: the routes are reported where they are written.
            [] => (rooted, true),
            // One implementation serves them — the row is its, unless it cannot (the method is not
            // declared there). Judged here only if the class keeps the interface's path and does not
            // re-annotate the method, since either would make these annotations dead.
            [only] => (
                rooted && !only.methods.contains(&sig),
                !only.has_class_path && !only.reannotated.contains(&sig),
            ),
            _ => (rooted, false),
        };
        out.push(rm);
    }
}

fn class_methods(unit: Src<'_>, t: &TypeFacts, env: &Env<'_>, out: &mut Vec<ResourceMethod>) {
    let pair = paired_interface(unit, t, env);
    let class = class_level(unit, t, pair);
    // An abstract class is never instantiated by the runtime; its subclasses decide what it serves.
    let listed = class.path.is_written() && !t.is_abstract;
    for m in t.methods.iter().filter(|m| !m.is_constructor) {
        let entry = if is_resource_method(m, unit, env.custom_verbs) {
            let sides = Sides { site: unit, site_type: t, site_method: m, source: unit, source_method: m };
            Some(build(&sides, &class, env.custom_verbs, Site::Declared))
        } else {
            inherited(unit, t, m, pair, &class, env)
        };
        if let Some(mut rm) = entry {
            rm.listed = listed;
            rm.resolved = !t.is_abstract;
            out.push(rm);
        }
    }
}

/// The route an unannotated implementation method takes from its interface.
fn inherited(
    unit: Src<'_>,
    t: &TypeFacts,
    m: &MethodFacts,
    pair: Option<(Src<'_>, &TypeFacts)>,
    class: &ClassLevel,
    env: &Env<'_>,
) -> Option<ResourceMethod> {
    let (iu, it) = pair?;
    if known::has_method_annotations(m, unit.facts, env.custom_verbs) {
        return None;
    }
    let mut same = it.methods.iter().filter(|im| im.name == m.name && im.params.len() == m.params.len());
    let im = same.next()?;
    if same.next().is_some() || !is_resource_method(im, iu, env.custom_verbs) {
        return None;
    }
    let sides = Sides { site: unit, site_type: t, site_method: m, source: iu, source_method: im };
    Some(build(&sides, class, env.custom_verbs, Site::Inherited))
}

/// The annotated interface a class takes its routes from — only when it implements exactly one and
/// no other project class implements it too.
fn paired_interface<'e>(unit: Src<'_>, t: &TypeFacts, env: &Env<'e>) -> Option<(Src<'e>, &'e TypeFacts)> {
    let mut found = None;
    for written in &t.implements {
        let Some((iu, it)) = resolve_interface(written, unit, &env.interfaces, env.custom_verbs) else {
            continue;
        };
        let shared = env.implementors.get(&it.fqcn).is_some_and(|v| v.iter().any(|i| i.fqcn != t.fqcn));
        if shared || found.is_some() {
            return None;
        }
        found = Some((iu, it));
    }
    found
}

/// What a class contributes to each of its routes.
struct ClassLevel {
    path: PathSpec,
    produces: String,
    consumes: String,
}

fn class_level(unit: Src<'_>, t: &TypeFacts, pair: Option<(Src<'_>, &TypeFacts)>) -> ClassLevel {
    let path = class_annotation(unit, t, pair, "Path").map_or(PathSpec::Absent, |(u, a)| written_path(u, a));
    let media = |name: &str| class_annotation(unit, t, pair, name).map(|(u, a)| known::media_text(a, u.text));
    ClassLevel {
        path,
        produces: media("Produces").unwrap_or_default(),
        consumes: media("Consumes").unwrap_or_default(),
    }
}

/// A class-level annotation: the class's own, else its paired interface's.
fn class_annotation<'a>(
    unit: Src<'a>,
    t: &'a TypeFacts,
    pair: Option<(Src<'a>, &'a TypeFacts)>,
    name: &str,
) -> Option<(Src<'a>, &'a AnnFacts)> {
    known::find(&t.annotations, unit.facts, name).map(|a| (unit, a)).or_else(|| {
        let (pu, pt) = pair?;
        known::find(&pt.annotations, pu.facts, name).map(|a| (pu, a))
    })
}

fn written_path(unit: Src<'_>, ann: &AnnFacts) -> PathSpec {
    match known::sole_literal(ann, unit.text) {
        Some(s) => PathSpec::Literal(lit(unit, s)),
        None => PathSpec::Opaque,
    }
}

fn lit(unit: Src<'_>, s: &AnnString) -> Lit {
    Lit { value: s.value.clone(), file: unit.facts.file.clone(), start: s.start, end: s.end }
}

/// Where a row points, and where its annotations are read — the same method for a declaration,
/// the interface's for an inherited route.
struct Sides<'a> {
    site: Src<'a>,
    site_type: &'a TypeFacts,
    site_method: &'a MethodFacts,
    source: Src<'a>,
    source_method: &'a MethodFacts,
}

fn build(s: &Sides<'_>, class: &ClassLevel, custom: &[CustomVerb], site: Site) -> ResourceMethod {
    let source = s.source;
    let verb = known::verb_of(s.source_method, source.facts, custom);
    let path = known::find(&s.source_method.annotations, source.facts, "Path");
    let media = |name: &str, fallback: &str| {
        known::find(&s.source_method.annotations, source.facts, name)
            .map(|a| known::media_text(a, source.text))
            .unwrap_or_else(|| fallback.to_string())
    };
    let declared = site == Site::Declared;
    ResourceMethod {
        class_fqcn: s.site_type.fqcn.clone(),
        method: s.site_method.name.clone(),
        file: s.site.facts.file.clone(),
        offset: s.site_method.name_offset,
        line: line_at(s.site.text, s.site_method.name_offset),
        return_type: s.site_method.return_type.clone(),
        verb_span: verb.as_ref().filter(|_| declared).map(|(_, a)| (a.start, a.end)),
        verb: verb.map(|(v, _)| v),
        path_span: path.filter(|_| declared).map(|a| (a.start, a.end)),
        class_path: class.path.clone(),
        method_path: path.map_or(PathSpec::Absent, |a| written_path(source, a)),
        produces: media("Produces", class.produces.as_str()),
        consumes: media("Consumes", class.consumes.as_str()),
        params: params(s),
        site,
        in_interface: s.site_type.kind == "interface",
        is_public: s.site_method.is_public,
        listed: false,
        resolved: true,
    }
}

/// The site method's parameters, bound by the source method's annotations — the same list for a
/// declaration, the interface's (by position) for an inherited route.
fn params(s: &Sides<'_>) -> Vec<Param> {
    let facts = s.source.facts;
    s.site_method
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let annotated = s.source_method.params.get(i).unwrap_or(p);
            let (binding, bound) = match known::binding_of(&annotated.annotations, facts) {
                Some((ann, kind)) => (kind, known::sole_literal(ann, s.source.text).map(|v| lit(s.source, v))),
                None if known::is_certain_entity(&annotated.annotations) => ("body", None),
                None => ("arg", None),
            };
            Param {
                name: p.name.clone(),
                type_text: p.type_text.clone(),
                name_offset: p.name_offset,
                binding,
                bound,
                optional: known::find(&annotated.annotations, facts, "DefaultValue").is_some(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Unit;
    use bennu_facts::prelude::scan_java;

    fn unit(path: &str, src: &str) -> Unit {
        Unit { facts: scan_java(path, src).unwrap(), text: src.to_string() }
    }

    /// Extract every unit against every other, the way the index does.
    fn extract_all(units: &[Unit]) -> Vec<ResourceMethod> {
        let srcs: Vec<Src<'_>> = units.iter().map(Unit::src).collect();
        let interfaces: Vec<Src<'_>> = srcs
            .iter()
            .copied()
            .filter(|u| u.facts.types.iter().any(|t| is_resource_interface(*u, t, &[])))
            .collect();
        let impls = implementors(&srcs, &interfaces, &[]);
        let env = Env { interfaces, implementors: &impls, custom_verbs: &[] };
        srcs.iter().flat_map(|u| extract(*u, &env)).collect()
    }

    const CONTRACT: &str = "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"orders\") @Produces(\"application/json\")\npublic interface OrderApi {\n  @GET @Path(\"{id}\") Order find(@PathParam(\"id\") long id);\n}\n";

    #[test]
    fn an_unannotated_implementation_serves_its_interfaces_routes() {
        let impl_src = "package com.acme;\npublic class OrderResource implements OrderApi {\n  public Order find(long orderId) { return null; }\n}\n";
        let all = extract_all(&[unit("/p/OrderApi.java", CONTRACT), unit("/p/OrderResource.java", impl_src)]);
        let listed: Vec<&ResourceMethod> = all.iter().filter(|m| m.listed).collect();
        assert_eq!(listed.len(), 1, "one route, not one per file: {all:#?}");
        let r = listed[0];
        assert_eq!(r.file, "/p/OrderResource.java");
        assert_eq!(r.site, Site::Inherited);
        assert_eq!(r.full_path(&Default::default()).as_deref(), Some("/orders/{id}"));
        assert_eq!(r.produces, "application/json");
        assert_eq!(r.params[0].name, "orderId", "the implementation's name");
        assert_eq!(r.params[0].bound.as_ref().map(|l| l.value.as_str()), Some("id"), "the interface's binding");
        let declared = all.iter().find(|m| m.in_interface).unwrap();
        assert!(!declared.listed && declared.resolved, "still checked where it is written");
    }

    #[test]
    fn an_interface_with_no_implementation_is_reported_where_it_is_written() {
        let all = extract_all(&[unit("/p/OrderApi.java", CONTRACT)]);
        assert_eq!(all.len(), 1);
        assert!(all[0].listed && all[0].in_interface && all[0].resolved);
    }

    #[test]
    fn two_implementations_leave_the_pair_unresolved() {
        let a = "package com.acme;\npublic class A implements OrderApi { public Order find(long id) { return null; } }\n";
        let b = "package com.acme;\npublic class B implements OrderApi { public Order find(long id) { return null; } }\n";
        let all = extract_all(&[unit("/p/OrderApi.java", CONTRACT), unit("/p/A.java", a), unit("/p/B.java", b)]);
        assert_eq!(all.len(), 1, "neither class inherits: {all:#?}");
        assert!(all[0].in_interface && all[0].listed && !all[0].resolved);
    }

    #[test]
    fn one_jaxrs_annotation_on_the_implementation_discards_the_interfaces() {
        let impl_src = "package com.acme;\nimport javax.ws.rs.Produces;\npublic class OrderResource implements OrderApi {\n  @Produces(\"text/plain\") public Order find(long id) { return null; }\n}\n";
        let all = extract_all(&[unit("/p/OrderApi.java", CONTRACT), unit("/p/OrderResource.java", impl_src)]);
        assert!(all.iter().all(|m| m.in_interface), "the implementation serves nothing: {all:#?}");
        assert!(!all[0].resolved, "and the interface's annotations are dead, so not judged");
    }

    #[test]
    fn a_sub_resource_locator_has_no_verb_and_a_class_without_path_is_not_a_root() {
        let src = "package com.acme;\nimport javax.ws.rs.*;\n@Path(\"orders\") public class Orders {\n  @Path(\"{id}/items\") public Items items() { return null; }\n}\nclass Items { @GET public String list() { return null; } }\n";
        let all = extract_all(&[unit("/p/Orders.java", src)]);
        let locator = all.iter().find(|m| m.method == "items").unwrap();
        assert_eq!(locator.kind(), "LOCATOR");
        assert!(locator.listed);
        let sub = all.iter().find(|m| m.method == "list").unwrap();
        assert!(!sub.listed, "reached only through the locator");
    }
}
