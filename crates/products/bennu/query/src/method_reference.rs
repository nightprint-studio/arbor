//! Completion after `::` — the member half of a **method reference**.
//!
//! `opt.map(ResolvedIdentity::|)` is a member access written with a different separator, and it
//! used to get no answer of its own: the member query scans back over the word to a `.`, found a
//! `:`, and declined — so the popup fell through to the bare-word answer, every local in scope and
//! every class on the classpath, at the one caret where only a method of `ResolvedIdentity` can be
//! written.
//!
//! ## What is offered
//!
//! The qualifier is read exactly as a `.` receiver is (see `completion::resolve_receiver`), and its
//! members through the same walk, visibility and ranking. What differs is which members a reference
//! can name, and what accepting one writes:
//!
//! * **methods only** — a field is not a function;
//! * **`new`**, through a type that can be instantiated — the constructor reference;
//! * **the name alone**, never `name()`. A reference names the method; parentheses would call it.
//!
//! ## What is ranked first
//!
//! The ones that COMPILE in the slot. `map(Function<? super T, ? extends U>)` over an
//! `Optional<ResolvedIdentity>` takes the instance methods of `ResolvedIdentity` with no parameters
//! and the statics that take one `ResolvedIdentity` — see [`rank::ReferenceShape`]. Everything else
//! stays in the list, below them: the fit is by name, and hiding on a miss would hide a subtype.

use std::collections::HashSet;

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::{enclosing_type_binary, functional_descriptor_at, TypeRef, TypeResolver};
use bennu_proto::prelude::CompletionItem;

use crate::completion::{
    collapse_overloads, collect_members, resolve_receiver, sort_ranked, split_prefix, Ranked,
    TypeNameCatalog, SITE_PLACEHOLDER,
};
use crate::rank;
use crate::resolver::IndexResolver;

/// Where a method reference's member name is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodReferenceSite {
    /// Just past the qualifier — the offset of the first `:` once any whitespace before it is gone.
    pub qualifier_end: usize,
    /// Where the member name being written starts (the caret, when nothing is typed yet).
    pub name_start: usize,
}

/// Read the caret as the member half of a method reference: `Type::|`, `Type::na|`, `value::|`,
/// `this::|`, `call()::|`. `None` for anything else.
///
/// Text, not a parse: the state this is asked about is `Type::` followed by nothing, which is not
/// Java. A `::` with nothing that could be a qualifier in front of it is not a reference either.
pub fn method_reference_site(source: &str, caret: usize) -> Option<MethodReferenceSite> {
    let caret = caret.min(source.len());
    let (name_start, _) = split_prefix(source, caret);
    let head = source[..name_start].trim_end_matches([' ', '\t']);
    let qualifier = head.strip_suffix("::")?.trim_end_matches([' ', '\t']);
    let last = qualifier.chars().next_back()?;
    if !(last.is_alphanumeric() || matches!(last, '_' | '$' | '>' | ']' | ')')) {
        return None;
    }
    Some(MethodReferenceSite { qualifier_end: qualifier.len(), name_start })
}

/// The completions after a `::` at `caret`, or `None` when the caret is not after one — so the
/// caller knows to ask its other questions. `Some(empty)` means "a reference, and nothing to offer":
/// the caller must NOT fall back to a bare-word answer there.
pub fn method_reference_completion<M: CpMemberIndex>(
    source: &str,
    caret: usize,
    resolver: &IndexResolver<M>,
    catalog: Option<&dyn TypeNameCatalog>,
    case: MatchCase,
) -> Option<Vec<CompletionItem>> {
    let caret = caret.min(source.len());
    let site = method_reference_site(source, caret)?;
    let prefix = &source[site.name_start..caret];
    let typed = Typed::new(prefix, case);

    // Rewrite `Qualifier::pre|rest` as `Qualifier.|rest` — the shape the receiver reading already
    // understands, typed prefix excised, the text after the caret kept so the call around the
    // reference still parses.
    let mut repaired = String::with_capacity(source.len() + 2);
    repaired.push_str(&source[..site.qualifier_end]);
    repaired.push('.');
    let dot_offset = repaired.len();
    if source[caret..].starts_with('(') {
        repaired.push_str(SITE_PLACEHOLDER);
    }
    repaired.push_str(&source[caret..]);
    let Some(receiver) = resolve_receiver(&repaired, dot_offset, resolver, catalog) else {
        return Some(Vec::new());
    };

    // The class the caret is in, for which privates are visible — asked of a buffer where the
    // reference has a name, so it parses (see `completion_in` for why that matters).
    let sited = format!("{}{SITE_PLACEHOLDER}{}", &source[..caret], &source[caret..]);
    let site_type = enclosing_type_binary(&sited, caret);

    let shape = functional_descriptor_at(source, caret, resolver).map(|d| rank::ReferenceShape {
        params: d.params,
        returns: d.returns,
        qualifier: receiver.ty.binary_name.clone(),
        through_type: receiver.is_type,
    });
    // Never ranked as a type receiver: through `Type::`, an instance method is not the mistake it is
    // after `Type.` — it is the unbound reference, and often exactly the one wanted.
    let ctx = rank::Context::new(source, false).for_reference(shape);

    let mut out: Vec<Ranked> = Vec::new();
    let mut seen = HashSet::new();
    collect_members(resolver, &receiver.ty, typed, site_type.as_deref(), false, &ctx, &mut out, &mut seen);
    out.retain(|r| r.item.kind == "method");
    if receiver.is_type && typed.matches("new") {
        out.extend(constructor_reference(resolver, &receiver.ty, &ctx));
    }
    collapse_overloads(&mut out);
    // No expected type is set on this context, so every fit is `None` and the order is the score's.
    sort_ranked(&mut out);
    Some(
        out.into_iter()
            .map(|r| CompletionItem {
                // The name, not a call: `map(ResolvedIdentity::identifier())` does not compile.
                insert_text: None,
                snippet_stops: Vec::new(),
                auto_import: receiver.import.clone(),
                ..r.item
            })
            .collect(),
    )
}

/// `Type::new`, when `Type` can be instantiated at all — not an interface, an abstract class, an
/// enum or an annotation.
///
/// Fits the slot when a declared constructor takes the slot's parameters, or — for a class that
/// declares none, and so has only the implicit no-argument one — when the slot takes nothing.
fn constructor_reference<M: CpMemberIndex>(
    resolver: &IndexResolver<M>,
    ty: &TypeRef,
    ctx: &rank::Context,
) -> Option<Ranked> {
    let members = resolver.members_of(&ty.binary_name)?;
    let flags = &members.flags;
    if flags.is_interface || flags.is_abstract || flags.is_enum || flags.is_annotation {
        return None;
    }
    let constructors: Vec<_> = members.methods.iter().filter(|m| m.name == "<init>").collect();
    let fits = ctx.reference.as_ref().is_some_and(|shape| {
        if constructors.is_empty() {
            shape.params.is_empty()
        } else {
            constructors.iter().any(|c| rank::params_fit(&c.params, &shape.params))
        }
    });
    let simple = ty.binary_name.rsplit(['/', '$']).next().unwrap_or(&ty.binary_name);
    Some(Ranked {
        score: if fits { 45 } else { 0 },
        fit: rank::Fit::None,
        item: CompletionItem {
            label: "new".to_string(),
            kind: "constructor".to_string(),
            detail: Some(format!("{simple}::new")),
            owner: Some(ty.binary_name.clone()),
            ..Default::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(marked: &str) -> Option<MethodReferenceSite> {
        let caret = marked.find('|').expect("a caret marker");
        method_reference_site(&marked.replacen('|', "", 1), caret)
    }

    #[test]
    fn a_type_qualified_reference_with_nothing_typed_is_a_site() {
        let s = site("x.map(ResolvedIdentity::|)").expect("a reference");
        assert_eq!(s.qualifier_end, "x.map(ResolvedIdentity".len());
        assert_eq!(s.name_start, "x.map(ResolvedIdentity::".len());
    }

    #[test]
    fn a_partly_typed_member_name_is_a_site_that_starts_after_the_colons() {
        let s = site("x.map(ResolvedIdentity::ide|)").expect("a reference");
        assert_eq!(s.name_start, "x.map(ResolvedIdentity::".len());
    }

    /// `this::`, a value, a call and a generic type all qualify a reference.
    #[test]
    fn every_qualifier_shape_is_a_site() {
        for marked in ["run(this::|)", "run(resolver::|)", "run(find()::|)", "run(List<String>::|)", "run(a . b :: |)"] {
            assert!(site(marked).is_some(), "{marked}");
        }
    }

    /// A member access, a label, a ternary and a `::` with nothing before it are not references.
    #[test]
    fn what_is_not_a_reference_is_not_a_site() {
        for marked in ["order.na|", "cond ? a : b|", "case X: fo|", "::fo|", "(::|"] {
            assert!(site(marked).is_none(), "{marked}");
        }
    }
}
