//! The names in scope that **fit an argument slot**, the likeliest first.
//!
//! In `transfer(|` with `transfer(Account from, Account to, long amount)`, the popup used to list every name in
//! scope by how close it was to the caret. The call already says more: the slot takes an `Account`, it is called
//! `from`, and of the two `Account`s in scope only one is called anything like that. So the names whose type fits
//! the slot lead, ordered by how much their name resembles the parameter's, and a clear winner is preselected.
//!
//! Silent unless every overload that admits the position agrees on the slot's type (as for enum constants), and
//! for a slot of `Object`, which everything fits and so says nothing.

use bennu_classpath::prelude::MemberIndex as CpMemberIndex;
use bennu_complete::prelude::{MatchCase, Typed};
use bennu_java::prelude::{
    caret_is_static, enclosing_type_binary, extract_symbols, infer_node_type_cached, parse_java, visible_bindings, InferCache, TypeRef,
    TypeResolver,
};
use bennu_proto::prelude::CompletionItem;

use crate::completion::split_prefix;
use crate::enum_arguments::{same_word, words_of};
use crate::hints::argument_slot;
use crate::member_text::render_type;
use crate::rank;
use crate::resolver::IndexResolver;

/// Where a name was found; breaks ties between equally similar names (a parameter is what the method was handed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Origin {
    Parameter,
    Local,
    Field,
}

impl Origin {
    fn kind(self) -> &'static str {
        match self {
            Origin::Parameter => "parameter",
            Origin::Local => "variable",
            Origin::Field => "field",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub(crate) name: String,
    pub(crate) origin: Origin,
    pub(crate) ty: TypeRef,
    /// Byte offset of the declaration; the nearer (larger) one is the one just written.
    pub(crate) decl: usize,
}

/// How much `name` resembles the parameter it would be passed as: the whole name equal ignoring case, else the
/// number of its words the parameter's name shares (a plural counts).
pub(crate) fn similarity(param: &str, name: &str) -> u32 {
    if param.eq_ignore_ascii_case(name) {
        return 100;
    }
    let wanted = words_of(param);
    words_of(name).iter().filter(|w| wanted.iter().any(|p| same_word(p, w))).count() as u32 * 10
}

/// `cands` best first: by similarity to `param` (when the callee names it), then parameter before local before
/// field, then the nearest declaration. Returns each with its similarity.
pub(crate) fn order(param: Option<&str>, mut cands: Vec<Candidate>) -> Vec<(u32, Candidate)> {
    let mut scored: Vec<(u32, Candidate)> = cands.drain(..).map(|c| (param.map_or(0, |p| similarity(p, &c.name)), c)).collect();
    scored.sort_by(|(sa, a), (sb, b)| sb.cmp(sa).then(a.origin.cmp(&b.origin)).then(b.decl.cmp(&a.decl)));
    scored
}

/// Whether the front of an ordered list is a clear winner: the only name that fits, or one that resembles the
/// parameter more than any other does.
fn clear_winner(ordered: &[(u32, Candidate)]) -> bool {
    match ordered {
        [] => false,
        [_] => true,
        [(top, _), (next, _), ..] => *top > 0 && next < top,
    }
}

/// Every parameter, local and field of the enclosing class visible at `at` (inner names shadow outer ones), with
/// whether the caret is in a static context. No type or prefix filter: the callers apply their own.
pub(crate) fn scope_candidates(source: &str, at: usize, resolver: &dyn TypeResolver) -> Option<(Vec<Candidate>, bool)> {
    let sited = format!("{}{}{}", &source[..at], crate::completion::SITE_PLACEHOLDER, &source[at..]);
    let tree = parse_java(&sited)?;
    let root = tree.root_node();
    let symbols = extract_symbols(&sited);
    let statik = caret_is_static(&root, &sited, at);
    let mut cands: Vec<Candidate> = Vec::new();
    for b in visible_bindings(&root, &sited, &symbols, at, resolver) {
        // a lambda's parameter has no declared type, but the functional interface it is passed to says what it is
        let ty = match b.ty {
            Some(ty) => ty,
            None if is_lambda_parameter(&root, &b) && !b.name.starts_with('_') => match lambda_parameter_type(source, at, &b.name, resolver) {
                Some(ty) => ty,
                None => continue,
            },
            None => continue,
        };
        if cands.iter().any(|c| c.name == b.name) {
            continue;
        }
        let origin = if b.is_parameter { Origin::Parameter } else { Origin::Local };
        cands.push(Candidate { name: b.name, origin, ty, decl: b.decl });
    }
    if let Some(site) = enclosing_type_binary(&sited, at) {
        if let Some(members) = resolver.members_of(&site) {
            for f in &members.fields {
                if f.name.contains('$') || (statik && !f.is_static) || cands.iter().any(|c| c.name == f.name) {
                    continue;
                }
                cands.push(Candidate { name: f.name.clone(), origin: Origin::Field, ty: f.return_type.clone(), decl: 0 });
            }
        }
    }
    Some((cands, statik))
}

/// Whether the binding is a parameter of a lambda (`x -> …`, `(a, b) -> …`), as opposed to a method's or a catch's.
fn is_lambda_parameter(root: &tree_sitter::Node<'_>, b: &bennu_java::prelude::Binding) -> bool {
    b.is_parameter
        && root
            .descendant_for_byte_range(b.decl, b.decl)
            .and_then(|n| n.parent())
            .is_some_and(|p| matches!(p.kind(), "lambda_expression" | "inferred_parameters"))
}

/// The type of lambda parameter `name`, read the way a use of it would be: the name is written where the caret is and typed
/// by the same inference that types `x.` — which knows the functional interface the lambda is passed to.
fn lambda_parameter_type(source: &str, at: usize, name: &str, resolver: &dyn TypeResolver) -> Option<TypeRef> {
    let text = format!("{}{name}{}", &source[..at], &source[at..]);
    let tree = parse_java(&text)?;
    let root = tree.root_node();
    let node = root.descendant_for_byte_range(at, at + name.len())?;
    let symbols = extract_symbols(&text);
    infer_node_type_cached(&root, &text, &symbols, &node, resolver, &InferCache::new())
}

/// The untyped parameters of the lambdas the caret is inside, as `(name, declaration offset)`, innermost first.
///
/// Their type is not known (`x -> foo(|)`), so no slot can be said to fit them — and that is what makes them a guess
/// worth offering: a lambda's parameter is what the body is about, and a wrong one is one keystroke to change. A name
/// that starts with `_` is left out, since it says it is not used.
pub(crate) fn untyped_lambda_parameters(source: &str, at: usize, resolver: &dyn TypeResolver) -> Vec<(String, usize)> {
    let sited = format!("{}{}{}", &source[..at], crate::completion::SITE_PLACEHOLDER, &source[at..]);
    let Some(tree) = parse_java(&sited) else { return Vec::new() };
    let root = tree.root_node();
    let symbols = extract_symbols(&sited);
    let mut out: Vec<(String, usize)> = Vec::new();
    for b in visible_bindings(&root, &sited, &symbols, at, resolver) {
        if b.ty.is_some() || !b.is_parameter || b.name.starts_with('_') || out.iter().any(|(n, _)| *n == b.name) {
            continue;
        }
        // one whose type the interface gives is a candidate like any other (see `scope_candidates`), not a guess
        if is_lambda_parameter(&root, &b) && lambda_parameter_type(source, at, &b.name, resolver).is_none() {
            out.push((b.name, b.decl));
        }
    }
    out
}

/// The locals, parameters and fields of the enclosing class that fit the slot at `caret`, as completions — best
/// first, the clear winner preselected. Empty outside a call argument, when the slot is ambiguous or `Object`,
/// and after a `.` or `::`.
pub fn argument_locals<M: CpMemberIndex>(
    source: &str,
    caret: usize,
    resolver: &IndexResolver<M>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    let mut at = caret.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let (start, prefix) = split_prefix(source, at);
    let head = source[..start].trim_end();
    if head.ends_with('.') || head.ends_with("::") {
        return Vec::new();
    }
    let Some(slot) = argument_slot(source, at, resolver) else { return Vec::new() };
    if slot.ty.dims == 0 && slot.ty.binary_name == "java/lang/Object" {
        return Vec::new();
    }
    let Some((all, statik)) = scope_candidates(source, at, resolver) else { return Vec::new() };
    let ctx = rank::Context::new(source, statik).expecting(Some(slot.ty.clone()));
    let typed = Typed::new(&prefix, case);
    let cands: Vec<Candidate> = all.into_iter().filter(|c| typed.tier(&c.name).is_some() && ctx.fit(&c.ty, resolver) != rank::Fit::None).collect();
    let ordered = order(slot.name.as_deref(), cands);
    let preselect = clear_winner(&ordered);
    ordered
        .into_iter()
        .enumerate()
        .map(|(rank, (_, c))| CompletionItem {
            label: c.name,
            kind: c.origin.kind().to_string(),
            detail: Some(render_type(&c.ty)),
            preselect: preselect && rank == 0,
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, origin: Origin, decl: usize) -> Candidate {
        Candidate { name: name.to_string(), origin, ty: TypeRef::simple("z/Account"), decl }
    }

    #[test]
    fn the_name_that_resembles_the_parameter_leads() {
        let ordered = order(Some("from"), vec![cand("target", Origin::Local, 40), cand("fromAccount", Origin::Local, 10), cand("from", Origin::Parameter, 0)]);
        let names: Vec<&str> = ordered.iter().map(|(_, c)| c.name.as_str()).collect();
        assert_eq!(names, ["from", "fromAccount", "target"]);
        assert!(clear_winner(&ordered));
    }

    #[test]
    fn without_resemblance_the_nearest_declaration_leads_and_nothing_is_preselected() {
        let ordered = order(Some("amount"), vec![cand("a", Origin::Local, 10), cand("b", Origin::Local, 30)]);
        assert_eq!(ordered[0].1.name, "b");
        assert!(!clear_winner(&ordered));
    }

    #[test]
    fn a_single_fitting_name_is_a_clear_winner_and_a_tie_is_not() {
        assert!(clear_winner(&order(None, vec![cand("only", Origin::Local, 1)])));
        assert!(!clear_winner(&order(Some("pin"), vec![cand("pinA", Origin::Local, 1), cand("pinB", Origin::Local, 2)])));
    }

    #[test]
    fn plurals_and_camel_words_count() {
        assert!(similarity("items", "item") >= 10);
        assert!(similarity("customerId", "id") >= 10);
        assert_eq!(similarity("amount", "Amount"), 100);
        assert_eq!(similarity("amount", "total"), 0);
    }
}
