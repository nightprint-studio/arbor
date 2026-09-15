//! Editor answers for a Java buffer: gutter marks, hover, go-to and completion.
//!
//! What they share with the diagnostics is the rule, not the bar. A gutter arrow or a go-to picker
//! shows every bean that *may* satisfy a point — the reader chooses, and a row the container would
//! not have picked costs a glance. Only the checks demand certainty.

use bennu_complete::prelude::{matches, Proposal, Proposals};
use bennu_ext::prelude::{ExtGutterMark, ExtHover, ExtTarget};
use bennu_proto::prelude::CompletionItem;

use crate::beans::{Bean, BeanOrigin};
use crate::inject::InjectionPoint;
use crate::known;
use crate::matching::{candidates, fit, resolution, Fit, Resolution};
use crate::model::Buffer;

/// How many bean names one completion offers.
const MAX_COMPLETIONS: usize = 200;

pub fn bean_target(b: &Bean) -> ExtTarget {
    ExtTarget { file: b.file.clone(), offset: b.offset, label: b.label(), detail: b.detail() }
}

pub fn point_target(p: &InjectionPoint) -> ExtTarget {
    ExtTarget { file: p.file.clone(), offset: p.offset, label: p.label(), detail: format!("{} · {}", p.type_text, p.kind.as_str()) }
}

/// The points in the project a bean may satisfy.
pub fn injected_into<'p>(buffer: &'p Buffer<'_>, bean: &Bean) -> Vec<&'p InjectionPoint> {
    buffer.points.iter().filter(|p| fit(&buffer.view, bean, p) != Fit::No).collect()
}

/// `inject` marks on injection points (not when Spring answers them), `bean` marks on beans.
pub fn gutter(buffer: &Buffer<'_>, spring: bool) -> Vec<ExtGutterMark> {
    let mut out = Vec::new();
    if !spring {
        let mode = buffer.model.gates().mode;
        for p in &buffer.own_points {
            let targets: Vec<ExtTarget> =
                candidates(&buffer.view, &buffer.beans, p, mode).into_iter().map(|c| bean_target(c.bean)).collect();
            let tooltip = match targets.as_slice() {
                [] => continue,
                [one] => format!("Injected with {}", one.label),
                many => format!("{} candidates", many.len()),
            };
            out.push(ExtGutterMark { line: p.line, kind: "inject".to_string(), tooltip, targets });
        }
    }
    let own = buffer.beans.iter().filter(|b| b.file == buffer.path && (b.explicit || b.origin != BeanOrigin::Class));
    for b in own {
        let targets: Vec<ExtTarget> =
            if spring { Vec::new() } else { injected_into(buffer, b).into_iter().map(point_target).collect() };
        let tooltip = match targets.len() {
            0 => format!("{} bean", b.badge),
            1 => "Injected into 1 place".to_string(),
            n => format!("Injected into {n} places"),
        };
        out.push(ExtGutterMark { line: b.line, kind: "bean".to_string(), tooltip, targets });
    }
    out
}

/// An injection point's candidates, or what a scope or bean-kind annotation means.
pub fn hover(buffer: &Buffer<'_>, offset: usize, spring: bool) -> Option<ExtHover> {
    if !spring {
        if let Some(p) = buffer.own_points.iter().find(|p| p.covers(offset)) {
            return point_hover(buffer, p);
        }
    }
    annotation_hover(buffer, offset)
}

fn point_hover(buffer: &Buffer<'_>, p: &InjectionPoint) -> Option<ExtHover> {
    let gates = buffer.model.gates();
    let found = candidates(&buffer.view, &buffer.beans, p, gates.mode);
    let doc = match found.as_slice() {
        [] => match resolution(&buffer.view, &buffer.beans, p, gates) {
            Resolution::Unsatisfied => "No bean in this project can be injected here.".to_string(),
            // Nothing found and nothing certain: the bean may live somewhere this crate cannot see.
            _ => return None,
        },
        [one] => format!("Injected with `{}` — {}.", one.bean.label(), one.bean.detail()),
        many => format!(
            "{} candidates: {}.",
            many.len(),
            many.iter().map(|c| c.bean.label()).collect::<Vec<_>>().join(", ")
        ),
    };
    Some(ExtHover { title: p.member.clone(), signature: format!("{} · {}", p.type_text, p.kind.as_str()), doc })
}

fn annotation_hover(buffer: &Buffer<'_>, offset: usize) -> Option<ExtHover> {
    let facts = &buffer.unit.facts;
    let annotations = facts.types.iter().flat_map(|t| {
        t.annotations
            .iter()
            .chain(t.methods.iter().flat_map(|m| m.annotations.iter()))
            .chain(t.fields.iter().flat_map(|f| f.annotations.iter()))
    });
    for ann in annotations.filter(|a| offset >= a.start && offset <= a.end) {
        let names = known::NORMAL_SCOPES.iter().chain(["Dependent"].iter()).chain(known::EJB_KINDS.iter());
        for name in names {
            if !known::is(ann, facts, name) {
                continue;
            }
            let signature = if known::EJB_KINDS.contains(name) {
                "Enterprise bean"
            } else if known::NORMAL_SCOPES.contains(name) {
                "Normal scope — reached through a client proxy"
            } else {
                "Pseudo-scope — no proxy"
            };
            return Some(ExtHover {
                title: format!("@{}", ann.name),
                signature: signature.to_string(),
                doc: known::meaning(name)?.to_string(),
            });
        }
    }
    None
}

/// Go-to from an injection point's name to the beans that may satisfy it.
pub fn navigate(buffer: &Buffer<'_>, offset: usize, spring: bool) -> Vec<ExtTarget> {
    if spring {
        return Vec::new();
    }
    let Some(p) = buffer.own_points.iter().find(|p| p.covers(offset)) else { return Vec::new() };
    candidates(&buffer.view, &buffer.beans, p, buffer.model.gates().mode).into_iter().map(|c| bean_target(c.bean)).collect()
}

/// Bean names inside the `@Named("…")` of an injection point.
pub fn completions(buffer: &Buffer<'_>, offset: usize, spring: bool) -> Vec<CompletionItem> {
    if spring {
        return Vec::new();
    }
    let Some((start, end)) = named_string_at(buffer, offset) else { return Vec::new() };
    let Some(typed) = buffer.unit.text.get(start..offset) else { return Vec::new() };
    let mut named: Vec<(&str, &Bean)> =
        buffer.beans.iter().filter(|b| b.injectable).filter_map(|b| Some((b.quals.named.as_deref()?, b))).collect();
    named.sort_by(|a, b| a.0.cmp(b.0));
    let mut proposals = Proposals::new(MAX_COMPLETIONS);
    for (name, bean) in named {
        if matches(typed, name) {
            proposals.offer(Proposal::new(name, "bean").detail(bean.detail()));
        }
    }
    proposals
        .into_items()
        .into_iter()
        .map(|item| CompletionItem { replace_start: Some(start), replace_end: Some(end), ..item })
        .collect()
}

/// The span of the `@Named` string under the caret, when it names an injection point's bean.
fn named_string_at(buffer: &Buffer<'_>, offset: usize) -> Option<(usize, usize)> {
    let facts = &buffer.unit.facts;
    for t in &facts.types {
        let fields = t
            .fields
            .iter()
            .filter(|f| known::has(&f.annotations, facts, "Inject") || known::has(&f.annotations, facts, "EJB"))
            .map(|f| &f.annotations);
        let params = t
            .methods
            .iter()
            .filter(|m| known::has(&m.annotations, facts, "Inject"))
            .flat_map(|m| m.params.iter().map(|p| &p.annotations));
        for anns in fields.chain(params) {
            for ann in anns.iter().filter(|a| known::is(a, facts, "Named")) {
                let hit = ann
                    .strings
                    .iter()
                    .find(|s| (s.element.is_empty() || s.element == "value") && offset >= s.start && offset <= s.end);
                if let Some(s) = hit {
                    return Some((s.start, s.end));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::Project;

    const USE: &str = "/p/a/Use.java";
    const IMPL: &str = "/p/a/Impl.java";

    fn project(use_src: &str) -> Project {
        Project::cdi(&[
            ("/p/a/Svc.java", "package a;\npublic interface Svc {}\n"),
            (IMPL, "package a;\n@Named @ApplicationScoped public class Impl implements Svc {}\n"),
            (USE, use_src),
        ])
    }

    const PLAIN_USE: &str = "package a;\n@RequestScoped public class Use {\n  @Inject Svc svc;\n}\n";

    #[test]
    fn an_injection_point_is_marked_with_its_bean_and_the_bean_with_its_points() {
        let p = project(PLAIN_USE);
        let b = p.model.buffer(USE, p.text(USE)).unwrap();
        let marks = gutter(&b, false);
        let inject = marks.iter().find(|m| m.kind == "inject").expect("inject mark");
        assert_eq!((inject.line, inject.tooltip.as_str()), (3, "Injected with Impl"));
        assert_eq!(inject.targets[0].file, IMPL);

        let b = p.model.buffer(IMPL, p.text(IMPL)).unwrap();
        let bean = gutter(&b, false).into_iter().find(|m| m.kind == "bean").expect("bean mark");
        assert_eq!(bean.tooltip, "Injected into 1 place");
        assert_eq!(bean.targets[0].label, "Use.svc");
    }

    #[test]
    fn spring_owns_the_injection_answers() {
        let p = project(PLAIN_USE);
        let b = p.model.buffer(USE, p.text(USE)).unwrap();
        assert!(gutter(&b, true).iter().all(|m| m.kind != "inject"));
        let at = p.text(USE).find("svc;").unwrap();
        assert!(navigate(&b, at, true).is_empty());
        assert!(hover(&b, at, true).is_none());
    }

    #[test]
    fn go_to_and_hover_on_a_point_name_its_candidates() {
        let p = project(PLAIN_USE);
        let b = p.model.buffer(USE, p.text(USE)).unwrap();
        let at = p.text(USE).find("svc;").unwrap() + 1;
        assert_eq!(navigate(&b, at, false).iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["Impl"]);
        assert!(hover(&b, at, false).unwrap().doc.contains("Injected with `Impl`"));
    }

    #[test]
    fn hover_on_a_scope_says_what_it_means() {
        let p = project(PLAIN_USE);
        let b = p.model.buffer(USE, p.text(USE)).unwrap();
        let at = p.text(USE).find("@RequestScoped").unwrap() + 2;
        let h = hover(&b, at, false).unwrap();
        assert_eq!(h.title, "@RequestScoped");
        assert!(h.doc.contains("per request"));
    }

    #[test]
    fn named_completes_bean_names_over_the_whole_string() {
        let src = "package a;\n@RequestScoped public class Use {\n  @Inject @Named(\"imxx\") Svc svc;\n}\n";
        let p = project(src);
        let b = p.model.buffer(USE, p.text(USE)).unwrap();
        let text = p.text(USE);
        let start = text.find("imxx").unwrap();
        let items = completions(&b, start + 2, false);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["impl"]);
        assert_eq!((items[0].replace_start, items[0].replace_end), (Some(start), Some(start + 4)));
        assert!(completions(&b, text.find("Svc svc").unwrap(), false).is_empty(), "outside the string");
    }
}
