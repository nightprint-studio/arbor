//! Completion, go-to and hover inside a `target` / `source` string.
//!
//! A mapping path is Java code MapStruct compiles later, written as a string the Java editor sees as
//! opaque. These make it behave like the code it is: the next property offered after the dot,
//! Ctrl+B to its declaration, its type under the mouse.

use bennu_complete::prelude::{Proposal, Proposals};
use bennu_ext::prelude::{ExtHover, ExtTarget};
use bennu_proto::prelude::CompletionItem;

use crate::annotations::{Elem, Lit};
use crate::mapper::{Mapper, MappingMethod};
use crate::paths::{segments, source_root, type_at, Segment, Side, SourceRoot};
use crate::properties::{Access, Property};
use crate::table::{TypeRef, TypeTable};

const MAX_COMPLETIONS: usize = 200;

struct Caret<'m> {
    method: &'m MappingMethod,
    lit: &'m Lit,
    side: Side,
}

fn caret_at(mappers: &[Mapper], offset: usize) -> Option<Caret<'_>> {
    for method in mappers.iter().flat_map(|m| m.methods.iter()) {
        for mapping in &method.anns.mappings {
            for (elem, side) in [(&mapping.target, Side::Target), (&mapping.source, Side::Source)] {
                if let Elem::Literal(lit) = elem {
                    if lit.start <= offset && offset <= lit.end {
                        return Some(Caret { method, lit, side });
                    }
                }
            }
        }
    }
    None
}

/// Where the path starts, and how many leading segments name a parameter rather than a property.
fn root(caret: &Caret<'_>, table: &TypeTable, segs: &[Segment<'_>]) -> Option<(TypeRef, usize)> {
    match caret.side {
        Side::Target => Some((caret.method.target.clone(), 0)),
        Side::Source => match source_root(&caret.method.sources, table, segs) {
            SourceRoot::Param { param, skip } => Some((param.ty.clone(), skip)),
            SourceRoot::Silent => None,
        },
    }
}

fn segment_at(segs: &[Segment<'_>], offset: usize) -> Option<usize> {
    segs.iter().position(|s| s.start <= offset && offset <= s.end)
}

/// The properties of whatever the path up to the caret leads to. Only the segment under the caret is
/// replaced, so accepting `street` in `address.st|` leaves `address.` alone.
pub fn completions(mappers: &[Mapper], table: &TypeTable, offset: usize) -> Vec<CompletionItem> {
    let Some(caret) = caret_at(mappers, offset) else { return Vec::new() };
    let segs = segments(caret.lit);
    let Some(index) = segment_at(&segs, offset) else { return Vec::new() };
    let seg = segs[index];
    let typed = &seg.text[..offset - seg.start];
    let mut out = Proposals::new(MAX_COMPLETIONS);

    let at_parameter = caret.side == Side::Source && index == 0;
    if at_parameter {
        for p in caret.method.sources.iter().filter(|p| prefixed(&p.name, typed)) {
            out.offer(Proposal::new(p.name.clone(), "parameter").detail(p.type_text.clone()));
        }
    }
    let start = if at_parameter {
        match caret.method.sources.as_slice() {
            [only] => Some((only.ty.clone(), 0)),
            _ => None,
        }
    } else {
        root(&caret, table, &segs)
    };
    if let Some((ty, skip)) = start.filter(|(_, skip)| *skip <= index) {
        if let TypeRef::Project(fqcn) = type_at(table, &ty, &segs[skip..index]) {
            if let Some(view) = table.view(&fqcn) {
                offer_properties(&mut out, &view.props, caret.side, typed);
            }
        }
    }
    out.into_items()
        .into_iter()
        .map(|item| CompletionItem { replace_start: Some(seg.start), replace_end: Some(seg.end), ..item })
        .collect()
}

/// The properties that fit the side first — writable for a target, readable for a source — then, for
/// a target, the read-only ones a nested path goes *through*.
fn offer_properties(out: &mut Proposals, props: &[Property], side: Side, typed: &str) {
    let first = |p: &Property| match side {
        Side::Target => p.writable != Access::No,
        Side::Source => p.readable != Access::No,
    };
    let passes: [&dyn Fn(&Property) -> bool; 2] = [&first, &|p: &Property| side.admits(p)];
    for pass in passes {
        for p in props.iter().filter(|p| pass(*p) && prefixed(&p.name, typed)) {
            out.offer(Proposal::new(p.name.clone(), "property").detail(p.type_text.clone()));
        }
    }
}

fn prefixed(name: &str, typed: &str) -> bool {
    name.to_lowercase().starts_with(&typed.to_lowercase())
}

/// The property a path segment under the caret names, when it names one.
fn property_at(mappers: &[Mapper], table: &TypeTable, offset: usize) -> Option<Property> {
    let caret = caret_at(mappers, offset)?;
    let segs = segments(caret.lit);
    let index = segment_at(&segs, offset)?;
    let (ty, skip) = root(&caret, table, &segs)?;
    if index < skip {
        return None;
    }
    let TypeRef::Project(fqcn) = type_at(table, &ty, &segs[skip..index]) else { return None };
    table.view(&fqcn)?.props.into_iter().find(|p| p.name == segs[index].text)
}

pub fn navigate(mappers: &[Mapper], table: &TypeTable, offset: usize) -> Vec<ExtTarget> {
    let Some(p) = property_at(mappers, table, offset) else { return Vec::new() };
    let Some(site) = p.primary_site() else { return Vec::new() };
    let Some(owner) = table.get(&site.owner) else { return Vec::new() };
    vec![ExtTarget {
        file: owner.file.clone(),
        offset: site.offset,
        label: format!("{}.{}", owner.name, p.name),
        detail: format!("{} · {}", p.type_text, site.origin.label()),
    }]
}

pub fn hover(mappers: &[Mapper], table: &TypeTable, offset: usize) -> Option<ExtHover> {
    let p = property_at(mappers, table, offset)?;
    let owner = p.owner.rsplit('.').next().unwrap_or(&p.owner);
    let origins: Vec<&str> = p.origins().into_iter().map(|o| o.label()).collect();
    Some(ExtHover {
        title: p.name.clone(),
        signature: format!("{owner}.{} : {}", p.name, p.type_text),
        doc: format!(
            "{} and {} — declared by {}.",
            access_word(p.readable, "readable"),
            access_word(p.writable, "writable"),
            origins.join(", ")
        ),
    })
}

fn access_word(access: Access, word: &str) -> String {
    match access {
        Access::Yes => word.to_string(),
        Access::Maybe => format!("possibly {word}"),
        Access::No => format!("not {word}"),
    }
}
