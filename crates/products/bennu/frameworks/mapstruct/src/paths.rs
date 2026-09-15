//! Dotted property paths — `target = "address.street"` — and the walk along them.
//!
//! The walk follows each segment's property type while it is a project type, and stops **silently**
//! the moment it is not: `address` of a type from a jar, `street` of a `String`, an empty segment
//! half-typed. Only a segment that names nothing on a type known completely is a finding.

use crate::annotations::Lit;
use crate::mapper::SourceParam;
use crate::properties::{Access, Property};
use crate::table::{TypeRef, TypeTable};

/// Which end of a mapping a path is on — writing a property, or reading one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Target,
    Source,
}

impl Side {
    /// Whether a property can plausibly be named on this side. Lenient on purpose: a completion or a
    /// suggestion that leaves out a property MapStruct would accept is worse than one extra entry.
    pub fn admits(self, p: &Property) -> bool {
        match self {
            Side::Target => p.writable != Access::No || p.readable != Access::No,
            Side::Source => p.readable != Access::No,
        }
    }
}

/// One segment of a path, with its absolute byte span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

pub fn segments(lit: &Lit) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (i, c) in lit.value.char_indices() {
        if c == '.' {
            out.push(segment(lit, from, i));
            from = i + 1;
        }
    }
    out.push(segment(lit, from, lit.value.len()));
    out
}

fn segment(lit: &Lit, from: usize, to: usize) -> Segment<'_> {
    Segment { text: &lit.value[from..to], start: lit.start + from, end: lit.start + to }
}

/// How a walk ended.
#[derive(Debug, Clone)]
pub enum Walk {
    /// Every segment named a property — one per segment.
    Resolved(Vec<Property>),
    /// Segment `index` names nothing on a type known completely, whose properties are `candidates`
    /// and whose simple name is `owner`.
    Missing { index: usize, owner: String, candidates: Vec<Property> },
    /// The path left what is known. Nothing to say.
    Silent,
}

pub fn walk(table: &TypeTable, start: &TypeRef, segs: &[Segment<'_>]) -> Walk {
    let mut current = start.clone();
    let mut resolved = Vec::new();
    for (index, seg) in segs.iter().enumerate() {
        let TypeRef::Project(fqcn) = &current else { return Walk::Silent };
        let Some(view) = table.view(fqcn) else { return Walk::Silent };
        if seg.text.is_empty() {
            return Walk::Silent;
        }
        let found = view.props.iter().find(|p| p.name == seg.text).cloned();
        match found {
            Some(p) => {
                current = table.property_type(&p);
                resolved.push(p);
            }
            None if view.complete => {
                let owner = fqcn.rsplit('.').next().unwrap_or(fqcn).to_string();
                return Walk::Missing { index, owner, candidates: view.props };
            }
            None => return Walk::Silent,
        }
    }
    Walk::Resolved(resolved)
}

/// The type the path `segs` leads to — for offering the next segment. Completeness does not matter
/// here: an incomplete type still has the properties it has.
pub fn type_at(table: &TypeTable, start: &TypeRef, segs: &[Segment<'_>]) -> TypeRef {
    let mut current = start.clone();
    for seg in segs {
        let TypeRef::Project(fqcn) = &current else { return TypeRef::Unknown };
        let Some(view) = table.view(fqcn) else { return TypeRef::Unknown };
        let Some(p) = view.props.iter().find(|p| p.name == seg.text) else { return TypeRef::Unknown };
        current = table.property_type(p);
    }
    current
}

/// Where a `source` path starts: at which parameter, after how many segments.
#[derive(Debug, Clone, Copy)]
pub enum SourceRoot<'m> {
    Param { param: &'m SourceParam, skip: usize },
    Silent,
}

/// With several source parameters a path must start with one's name. With one, it may — unless that
/// parameter's type also has a property of the same name, where MapStruct's choice is not guessed.
pub fn source_root<'m>(sources: &'m [SourceParam], table: &TypeTable, segs: &[Segment<'_>]) -> SourceRoot<'m> {
    let first = segs.first().map(|s| s.text).unwrap_or("");
    if let Some(param) = sources.iter().find(|p| p.name == first) {
        if sources.len() == 1 && has_property(table, &param.ty, first) {
            return SourceRoot::Silent;
        }
        return SourceRoot::Param { param, skip: 1 };
    }
    match sources {
        [only] => SourceRoot::Param { param: only, skip: 0 },
        _ => SourceRoot::Silent,
    }
}

fn has_property(table: &TypeTable, ty: &TypeRef, name: &str) -> bool {
    match ty {
        TypeRef::Project(fqcn) => table.view(fqcn).is_some_and(|v| v.props.iter().any(|p| p.name == name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_carry_absolute_spans() {
        let lit = Lit { value: "address.street".into(), start: 10, end: 24 };
        let segs = segments(&lit);
        assert_eq!(segs.len(), 2);
        assert_eq!((segs[1].text, segs[1].start, segs[1].end), ("street", 18, 24));
    }

    #[test]
    fn a_trailing_dot_leaves_an_empty_segment_and_the_walk_goes_quiet() {
        let lit = Lit { value: "address.".into(), start: 0, end: 8 };
        let segs = segments(&lit);
        assert_eq!(segs[1].text, "");
        assert!(matches!(walk(&TypeTable::default(), &TypeRef::Unknown, &segs), Walk::Silent));
    }
}
