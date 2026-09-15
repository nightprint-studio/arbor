//! Overload applicability — which of a method's same-named declarations a call binds to.
//!
//! JLS §15.12.2 in miniature, and the ONE copy of it: the inference walk narrows a return type with
//! it, lambda-parameter typing picks the functional interface with it, and go-to / hover pick the
//! declaration to show with it ([`call_overload_at`]). Before they shared it, go-to landed on the
//! first declaration of the name — `client.post().uri(b -> …)` opened `uri(URI)`, because nothing
//! asked what a lambda can be passed to.
//!
//! The rules, in order:
//!
//! 1. **Arity**, varargs-aware (a trailing array parameter soaks up any count from its fixed prefix).
//! 2. **Applicability** by phase — strict (no boxing), then loose (boxing), then varargs — taking the
//!    first phase anything survives, as javac does. Per argument:
//!    * a **lambda** fits only a functional interface, and one whose single abstract method takes as
//!      many parameters as the lambda declares (`x -> …` one, `(a, b) -> …` two, `() -> …` none);
//!    * a **method reference** fits any functional interface;
//!    * **`null`** fits any reference type;
//!    * a **typed** value fits by identity, primitive widening, (un)boxing in the loose phases, and
//!      subtyping read off the hierarchy;
//!    * an argument **nothing could type** fits everything — unknown never excludes.
//! 3. **Most specific** among the survivors, compared ONLY at positions whose argument is typed: a
//!    position the engine could not type makes two candidates incomparable rather than letting
//!    declaration order or parameter shape alone decide.
//!
//! Every verdict that needs the classpath abstains when the classpath cannot answer: an interface
//! whose hierarchy is not fully resolvable is neither functional nor not, and a subtype question
//! over an incomplete walk is "unknown". Abstaining keeps a candidate. The rules only ever remove
//! an overload the code could not have called.
//!
//! What a call being edited looks like is part of the contract. `uri(b -> b.path("/x").)` does not
//! parse cleanly: tree-sitter leaves the lambda whole or wraps its pieces in an `ERROR` node, and a
//! stray `.` can become an argument of its own. The argument list drops punctuation-only errors
//! ([`Ctx::call_arg_nodes`]) and a lambda is still recognised by its `->` inside an error.

use std::cell::Cell;

use tree_sitter::Node;

use super::{
    arity_admits, boxed, enclosing_type_fqn, is_primitive, last_is_array, to_binary, unbox, Ctx,
    InferCache,
};
use crate::seam::{Member, TypeRef, TypeResolver};
use crate::symbols::{node_text, FileSymbols};

/// The overload the call whose NAME is under `byte_offset` binds to, or `None` when the caret is on
/// no call, its receiver cannot be typed, or the rules leave more than one candidate standing.
///
/// `None` is an answer, not a failure: the caller keeps whatever fallback it had (arity, then the
/// first declaration), which is what an ambiguous call deserves.
pub fn call_overload_at(
    source: &str,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<Member> {
    let tree = crate::grammar::parse_java(source)?;
    let root = tree.root_node();
    let call = call_named_at(&root, byte_offset)?;
    let symbols = crate::symbols::extract_symbols(source);
    let bytes = source.as_bytes();
    let cache = InferCache::new();
    let ctx = Ctx {
        root,
        bytes,
        resolver,
        symbols: &symbols,
        cache: &cache,
        depth: Cell::new(0),
    };
    let enclosing = enclosing_type_fqn(&call, bytes, &symbols);
    let args = ctx.call_arg_nodes(&call);
    let candidates = ctx.call_candidates(&call, &args, enclosing.as_deref())?;
    ctx.bind(&candidates, &args, enclosing.as_deref()).cloned()
}

/// The overload among `candidates` that `call` binds to — [`call_overload_at`] for a caller that
/// already holds the call node and its candidate set (parameter hints, the signature strip).
///
/// `call` is a `method_invocation` or an `object_creation_expression`; only its `arguments` are read.
/// `candidates` may repeat a signature (an override seen at several hierarchy levels) — the first
/// occurrence is kept. A lone signature is trusted whatever the arity; otherwise the rules in the
/// module doc decide, and more than one survivor is `None`.
pub fn bound_overload<'m>(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    call: &Node,
    candidates: &'m [Member],
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<&'m Member> {
    let bytes = source.as_bytes();
    let ctx = Ctx { root: *root, bytes, resolver, symbols, cache, depth: Cell::new(0) };
    let enclosing = enclosing_type_fqn(call, bytes, symbols);
    let args = ctx.call_arg_nodes(call);
    ctx.bind(candidates, &args, enclosing.as_deref())
}

/// What the arguments of a call make of its overload set. See [`overload_fit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverloadFit<'m> {
    /// No candidate's parameter count admits the call (varargs-aware).
    NoArity,
    /// The candidates the arguments are applicable to, most-specific kept. One means the call binds
    /// to it; several means the rules could not tell them apart.
    Applicable(Vec<&'m Member>),
    /// Arity admits these (distinct signatures), but no phase accepts any of them. Proven only on
    /// evidence — an argument or a type the classpath cannot read never lands here.
    Inapplicable(Vec<&'m Member>),
}

/// The full verdict of the applicability rules on `call` against `candidates` — for a caller that
/// has to know the difference between "ambiguous" and "nothing can be called", which
/// [`bound_overload`] collapses. Unlike it, a lone candidate is judged too.
pub fn overload_fit<'m>(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    call: &Node,
    candidates: &'m [Member],
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> OverloadFit<'m> {
    let bytes = source.as_bytes();
    let ctx = Ctx { root: *root, bytes, resolver, symbols, cache, depth: Cell::new(0) };
    let enclosing = enclosing_type_fqn(call, bytes, symbols);
    let args = ctx.call_arg_nodes(call);
    let distinct = distinct_signatures(candidates);
    ctx.fit_overloads(&distinct, &args, enclosing.as_deref())
}

/// The `method_invocation` whose name token holds `offset`. A caret inside an argument belongs to
/// the innermost call around it only when it is on THAT call's name — never to the enclosing one.
fn call_named_at<'t>(root: &Node<'t>, offset: usize) -> Option<Node<'t>> {
    let mut cur = root.named_descendant_for_byte_range(offset, offset);
    while let Some(n) = cur {
        if n.kind() == "method_invocation" {
            if let Some(name) = n.child_by_field_name("name") {
                if name.start_byte() <= offset && offset <= name.end_byte() {
                    return Some(n);
                }
            }
        }
        cur = n.parent();
    }
    None
}

/// One member per parameter signature, first occurrence kept.
///
/// The same method reachable at several hierarchy levels — an inherit, or a covariant override —
/// arrives several times with identical parameters. `resolve_methods` visits the receiver's own
/// members first, so keeping the first keeps the most derived.
pub(super) fn distinct_signatures(candidates: &[Member]) -> Vec<&Member> {
    let mut out: Vec<&Member> = Vec::new();
    for m in candidates {
        if !out.iter().any(|d| d.params == m.params) {
            out.push(m);
        }
    }
    out
}

/// What an argument contributes to applicability, read once per call.
enum Arg {
    /// A lambda, with its declared parameter count when the parameter list could be read.
    Lambda(Option<usize>),
    /// `Type::method`, `value::method`, `Type::new`.
    MethodRef,
    /// The `null` literal.
    Null,
    /// Any other expression, with its type when inference could say.
    Value(Option<TypeRef>),
}

/// The three invocation phases of JLS §15.12.2.2-4.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Fixed arity, no boxing.
    Strict,
    /// Fixed arity, boxing and unboxing allowed.
    Loose,
    /// A trailing array parameter taking the variadic arguments one element at a time.
    Varargs,
}

/// What a parameter type is to a lambda or a method reference.
enum Target {
    /// A functional interface whose single abstract method takes this many parameters.
    Functional(usize),
    /// Definitely not a functional interface — a class, a primitive, an array, an interface with
    /// zero or several abstract methods.
    NotFunctional,
    /// The classpath cannot say (a type variable, an unresolvable type or hierarchy).
    Unknown,
}

/// A subtype question's answer, with room for "the hierarchy is not fully known".
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Yes,
    No,
    Unknown,
}

impl Ctx<'_> {
    /// Every method named like `call` on the type it is called on: the receiver's type, else — for a
    /// bare call — the enclosing type, else the static import that can take this many arguments.
    /// The same order [`Ctx::infer_method_invocation`] resolves a call in.
    fn call_candidates(
        &self,
        call: &Node,
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<Vec<Member>> {
        let name = node_text(&call.child_by_field_name("name")?, self.bytes)?;
        if let Some(obj) = call.child_by_field_name("object") {
            let recv = self
                .infer_expr(&obj, enclosing)
                .or_else(|| self.type_receiver(&obj))?;
            return self.methods_named(&recv.binary_name, &name);
        }
        if let Some(found) = enclosing.and_then(|fqn| self.methods_named(&to_binary(fqn), &name)) {
            return Some(found);
        }
        let targets = crate::static_import::static_import_targets(&self.symbols.imports);
        let named = targets.iter().filter(|t| t.member.as_deref() == Some(name.as_str()));
        let starred = targets.iter().filter(|t| t.member.is_none());
        named
            .chain(starred)
            .filter(|t| self.declares_at_arity(&t.owner_binary, &name, args.len()))
            .find_map(|t| self.methods_named(&t.owner_binary, &name))
    }

    /// The methods named `name` across `binary`'s hierarchy, or `None` when there are none.
    pub(super) fn methods_named(&self, binary: &str, name: &str) -> Option<Vec<Member>> {
        let found = self.cache.resolve_methods(self.resolver, binary, name);
        (!found.candidates.is_empty()).then(|| found.candidates.clone())
    }

    /// The one overload of `candidates` a call passing `args` binds to: a lone signature whatever the
    /// arity, else the single survivor of [`Ctx::narrow_overloads`].
    fn bind<'m>(
        &self,
        candidates: &'m [Member],
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Option<&'m Member> {
        let distinct = distinct_signatures(candidates);
        if let [only] = distinct.as_slice() {
            return Some(*only);
        }
        match self.narrow_overloads(&distinct, args, enclosing).as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// The candidates a call passing `args` could bind to, by the rules in the module doc.
    ///
    /// Never empties a set arity admitted: when no candidate survives the argument rules, the model
    /// is what failed (a mistyped argument, a hierarchy read wrong), and the arity survivors are
    /// handed back for the caller's usual ambiguity handling.
    pub(super) fn narrow_overloads<'m>(
        &self,
        candidates: &[&'m Member],
        args: &[Node],
        enclosing: Option<&str>,
    ) -> Vec<&'m Member> {
        let admitted = admitted_by_arity(candidates, args.len());
        // A lone arity survivor needs no argument typed — the common case, kept cheap.
        if admitted.len() < 2 {
            return admitted;
        }
        match self.fit_overloads(&admitted, args, enclosing) {
            OverloadFit::NoArity => Vec::new(),
            OverloadFit::Applicable(kept) | OverloadFit::Inapplicable(kept) => kept,
        }
    }

    /// Arity, then the three phases, then most-specific — the whole rule set, with "nothing is
    /// applicable" kept distinct from "ambiguous". Judges a lone candidate too.
    fn fit_overloads<'m>(
        &self,
        candidates: &[&'m Member],
        args: &[Node],
        enclosing: Option<&str>,
    ) -> OverloadFit<'m> {
        let admitted = admitted_by_arity(candidates, args.len());
        if admitted.is_empty() {
            return OverloadFit::NoArity;
        }
        let args: Vec<Arg> = args.iter().map(|a| self.classify_arg(a, enclosing)).collect();
        for phase in [Phase::Strict, Phase::Loose, Phase::Varargs] {
            let applicable: Vec<&'m Member> = admitted
                .iter()
                .copied()
                .filter(|m| self.applicable_in(*m, &args, phase))
                .collect();
            if !applicable.is_empty() {
                return OverloadFit::Applicable(self.most_specific(applicable, &args, phase));
            }
        }
        OverloadFit::Inapplicable(admitted)
    }

    fn classify_arg(&self, arg: &Node, enclosing: Option<&str>) -> Arg {
        match arg.kind() {
            "lambda_expression" => Arg::Lambda(lambda_arity(arg)),
            "method_reference" => Arg::MethodRef,
            "null_literal" => Arg::Null,
            "string_literal" | "text_block" => {
                Arg::Value(Some(TypeRef::simple("java/lang/String")))
            }
            "character_literal" => Arg::Value(Some(TypeRef::simple("char"))),
            "true" | "false" => Arg::Value(Some(TypeRef::simple("boolean"))),
            "parenthesized_expression" => match arg.named_child(0) {
                Some(inner) => self.classify_arg(&inner, enclosing),
                None => Arg::Value(None),
            },
            "ERROR" => broken_lambda_arity(arg).map_or(Arg::Value(None), Arg::Lambda),
            _ => Arg::Value(self.infer_expr(arg, enclosing)),
        }
    }

    fn applicable_in(&self, m: &Member, args: &[Arg], phase: Phase) -> bool {
        let shape = match phase {
            Phase::Strict | Phase::Loose => m.params.len() == args.len(),
            Phase::Varargs => last_is_array(m) && args.len() + 1 >= m.params.len(),
        };
        shape
            && args.iter().enumerate().all(|(i, arg)| {
                param_for(m, i, phase).is_some_and(|p| self.arg_fits(arg, &p, phase))
            })
    }

    fn arg_fits(&self, arg: &Arg, param: &TypeRef, phase: Phase) -> bool {
        match arg {
            Arg::Lambda(arity) => match self.functional_target(param) {
                Target::Functional(sam_arity) => arity.map_or(true, |n| n == sam_arity),
                Target::NotFunctional => false,
                Target::Unknown => true,
            },
            Arg::MethodRef => !matches!(self.functional_target(param), Target::NotFunctional),
            Arg::Null => param.dims > 0 || !is_primitive(&param.binary_name),
            Arg::Value(None) => true,
            Arg::Value(Some(ty)) => self.value_fits(ty, param, phase != Phase::Strict),
        }
    }

    /// Whether `param` is a functional interface, and how many parameters its SAM takes.
    ///
    /// A class is decided by its own flags, with no walk — which is what lets `uri(URI)` be ruled out
    /// for a lambda even on a classpath whose hierarchy above `URI` is not indexed.
    fn functional_target(&self, param: &TypeRef) -> Target {
        if param.dims > 0 || is_primitive(&param.binary_name) {
            return Target::NotFunctional;
        }
        if self.is_type_variable(&param.binary_name) {
            return Target::Unknown;
        }
        let Some(members) = self.resolver.members_of(&param.binary_name) else {
            return Target::Unknown;
        };
        if !members.flags.is_interface {
            return Target::NotFunctional;
        }
        match self.abstract_methods(param).as_deref() {
            Some([sam]) => Target::Functional(sam.params.len()),
            Some(_) => Target::NotFunctional,
            None => Target::Unknown,
        }
    }

    /// Whether a value of type `arg` can be passed where `param` is declared. `boxing` is the loose
    /// phases' permission to (un)box.
    fn value_fits(&self, arg: &TypeRef, param: &TypeRef, boxing: bool) -> bool {
        if self.is_type_variable(&param.binary_name) || self.is_type_variable(&arg.binary_name) {
            return true;
        }
        let param_primitive = param.dims == 0 && is_primitive(&param.binary_name);
        let arg_primitive = arg.dims == 0 && is_primitive(&arg.binary_name);
        match (param_primitive, arg_primitive) {
            (true, true) => widens(&arg.binary_name, &param.binary_name),
            (false, false) => self.subtype(arg, param) != Verdict::No,
            (true, false) => {
                boxing && unbox(&arg.binary_name).is_some_and(|p| widens(p, &param.binary_name))
            }
            (false, true) => boxing && self.subtype(&boxed(arg.clone()), param) != Verdict::No,
        }
    }

    /// Whether `sub` is `sup` or a subtype of it, arrays included (JLS §4.10.3).
    fn subtype(&self, sub: &TypeRef, sup: &TypeRef) -> Verdict {
        subtype_of(self.resolver, sub, sup)
    }

    /// The applicable candidates no other applicable candidate is strictly more specific than.
    fn most_specific<'m>(
        &self,
        applicable: Vec<&'m Member>,
        args: &[Arg],
        phase: Phase,
    ) -> Vec<&'m Member> {
        if applicable.len() < 2 {
            return applicable;
        }
        let beaten = |m: &Member| {
            applicable.iter().any(|other| {
                !std::ptr::eq(*other, m)
                    && self.at_least_as_specific(other, m, args, phase)
                    && !self.at_least_as_specific(m, other, args, phase)
            })
        };
        let kept: Vec<&'m Member> = applicable.iter().copied().filter(|m| !beaten(*m)).collect();
        // An incomplete hierarchy can make "more specific" cyclic, which would beat everyone. The
        // answer then is that nothing was decided, not that nothing applies.
        if kept.is_empty() {
            applicable
        } else {
            kept
        }
    }

    /// Whether `a`'s parameters are each at least as specific as `b`'s — judged only where the
    /// argument is typed (or `null`). A lambda, a method reference or an untyped argument at a
    /// position where the two differ leaves them incomparable.
    fn at_least_as_specific(&self, a: &Member, b: &Member, args: &[Arg], phase: Phase) -> bool {
        (0..args.len()).all(|i| {
            let (Some(pa), Some(pb)) = (param_for(a, i, phase), param_for(b, i, phase)) else {
                return false;
            };
            if pa == pb {
                return true;
            }
            matches!(args[i], Arg::Value(Some(_)) | Arg::Null) && self.type_more_specific(&pa, &pb)
        })
    }

    fn type_more_specific(&self, a: &TypeRef, b: &TypeRef) -> bool {
        let a_primitive = a.dims == 0 && is_primitive(&a.binary_name);
        let b_primitive = b.dims == 0 && is_primitive(&b.binary_name);
        match (a_primitive, b_primitive) {
            (true, true) => widens(&a.binary_name, &b.binary_name),
            (false, false) => {
                !self.is_type_variable(&a.binary_name)
                    && !self.is_type_variable(&b.binary_name)
                    && self.subtype(a, b) == Verdict::Yes
            }
            _ => false,
        }
    }
}

/// The candidates whose parameter count admits a call of `argc` arguments, in order.
fn admitted_by_arity<'m>(candidates: &[&'m Member], argc: usize) -> Vec<&'m Member> {
    candidates
        .iter()
        .copied()
        .filter(|m| arity_admits(m.params.len(), last_is_array(m), argc))
        .collect()
}

/// Deep enough for any real class chain, finite because an index may hold a cycle.
const MAX_SUPERCLASS_CHAIN: usize = 64;

/// Whether `sub` is `sup` or a subtype of it — `None` when the classpath cannot say.
///
/// The subtype question the applicability rules ask, for a caller that needs the same answer one
/// position at a time (the argument-type check), so the two never disagree about a hierarchy.
pub fn subtype_verdict(resolver: &dyn TypeResolver, sub: &TypeRef, sup: &TypeRef) -> Option<bool> {
    match subtype_of(resolver, sub, sup) {
        Verdict::Yes => Some(true),
        Verdict::No => Some(false),
        Verdict::Unknown => None,
    }
}

/// Whether `sub` is `sup` or a subtype of it, arrays included (JLS §4.10.3).
fn subtype_of(resolver: &dyn TypeResolver, sub: &TypeRef, sup: &TypeRef) -> Verdict {
    if sup.dims == 0 && sup.binary_name == "java/lang/Object" {
        return Verdict::Yes;
    }
    if sub.dims > 0 || sup.dims > 0 {
        return match (sub.dims, sup.dims) {
            (_, 0) => verdict(matches!(
                sup.binary_name.as_str(),
                "java/lang/Cloneable" | "java/io/Serializable"
            )),
            (0, _) => Verdict::No,
            _ => subtype_of(resolver, &one_dim_less(sub), &one_dim_less(sup)),
        };
    }
    if crate::typename::same_binary_type(&sub.binary_name, &sup.binary_name) {
        return Verdict::Yes;
    }
    if is_primitive(&sub.binary_name) || is_primitive(&sup.binary_name) {
        return Verdict::No;
    }
    let walk = crate::hierarchy::walk(resolver, &TypeRef::simple(sub.binary_name.clone()), |a| {
        crate::typename::same_binary_type(&a.ty.binary_name, &sup.binary_name).then_some(())
    });
    match (walk.found, walk.complete) {
        (Some(()), _) => Verdict::Yes,
        (None, true) => Verdict::No,
        (None, false) => subtype_past_a_hole(resolver, &sub.binary_name, &sup.binary_name),
    }
}

/// `sub`'s hierarchy has a hole the walk could not read past: whether `sup` could hide behind it.
///
/// Only an interface can be reached through ANY supertype. A class is reached through the
/// superclass chain alone — an interface extends no class — and a `final` class through nothing at
/// all. So an unindexed `Serializable` above a DTO no longer makes that DTO "maybe" a `String` or a
/// `Token`, which left every call passing one where the other was wanted unjudged.
fn subtype_past_a_hole(resolver: &dyn TypeResolver, sub: &str, sup: &str) -> Verdict {
    let Some(sup_members) = resolver.members_of(sup) else { return Verdict::Unknown };
    if sup_members.flags.is_interface {
        return Verdict::Unknown;
    }
    if sup_members.flags.is_final {
        return Verdict::No;
    }
    let mut current = sub.to_string();
    for _ in 0..MAX_SUPERCLASS_CHAIN {
        if crate::typename::same_binary_type(&current, sup) {
            return Verdict::Yes;
        }
        let Some(members) = resolver.members_of(&current) else { return Verdict::Unknown };
        match &members.superclass {
            Some(next) => current = next.binary_name.clone(),
            None => return Verdict::No,
        }
    }
    Verdict::Unknown
}

fn verdict(yes: bool) -> Verdict {
    if yes {
        Verdict::Yes
    } else {
        Verdict::No
    }
}

/// The parameter argument `i` is checked against: its own, or — in the varargs phase, from the
/// trailing array on — that array's element type.
fn param_for(m: &Member, i: usize, phase: Phase) -> Option<TypeRef> {
    let last = m.params.len().checked_sub(1)?;
    if phase == Phase::Varargs && i >= last {
        return Some(one_dim_less(&m.params[last]));
    }
    m.params.get(i).cloned()
}

/// `ty` with one array dimension removed — also for the legacy `Foo[]` spelling of a binary name.
fn one_dim_less(ty: &TypeRef) -> TypeRef {
    let mut out = ty.clone();
    if out.dims > 0 {
        out.dims -= 1;
    } else if let Some(element) = out.binary_name.strip_suffix("[]") {
        out.binary_name = element.to_string();
    }
    out
}

/// Primitive widening (JLS §5.1.2), identity included.
fn widens(from: &str, to: &str) -> bool {
    from == to
        || matches!(
            (from, to),
            ("byte", "short" | "int" | "long" | "float" | "double")
                | ("short" | "char", "int" | "long" | "float" | "double")
                | ("int", "long" | "float" | "double")
                | ("long", "float" | "double")
                | ("float", "double")
        )
}

/// How many parameters a lambda declares, `None` when its parameter list is not one of the shapes
/// the grammar gives a lambda.
fn lambda_arity(lambda: &Node) -> Option<usize> {
    parameter_count(&lambda.child_by_field_name("parameters")?)
}

fn parameter_count(params: &Node) -> Option<usize> {
    match params.kind() {
        // `x -> …`, and `(x) -> …` where error recovery read the parentheses as an expression.
        "identifier" | "parenthesized_expression" => Some(1),
        "inferred_parameters" | "formal_parameters" => {
            let mut c = params.walk();
            let count = params
                .named_children(&mut c)
                .filter(|p| matches!(p.kind(), "identifier" | "formal_parameter" | "spread_parameter"))
                .count();
            Some(count)
        }
        _ => None,
    }
}

/// A lambda inside an `ERROR` node — the shape of one whose body is still being typed.
///
/// `None` when there is no lambda here at all. `Some(None)` when there is (a `->` says so) but its
/// parameter list could not be read: still a lambda, so still only a functional interface fits.
fn broken_lambda_arity(error: &Node) -> Option<Option<usize>> {
    let mut c = error.walk();
    let children: Vec<Node> = error.children(&mut c).collect();
    if let Some(lambda) = children.iter().find(|n| n.kind() == "lambda_expression") {
        return Some(lambda_arity(lambda));
    }
    let arrow = children.iter().position(|n| n.kind() == "->")?;
    let Some(before) = arrow.checked_sub(1).map(|i| children[i]) else {
        return Some(None);
    };
    if before.kind() != ")" {
        return Some(parameter_count(&before));
    }
    // `(a, b) ->` left as loose tokens: count the identifiers back to the opening parenthesis.
    let close = arrow - 1;
    let open = children[..close].iter().rposition(|n| n.kind() == "(");
    Some(open.map(|o| children[o + 1..close].iter().filter(|n| n.kind() == "identifier").count()))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::seam::ClassMembers;
    use std::collections::HashMap;

    fn interface(methods: Vec<Member>) -> ClassMembers {
        let mut members = cm(methods);
        members.flags.is_interface = true;
        members.flags.is_abstract = true;
        members
    }

    fn abstract_meth(name: &str, ret: &str, params: &[&str]) -> Member {
        let mut m = meth(name, ret, params);
        m.is_abstract = true;
        m
    }

    fn array_of(binary: &str) -> TypeRef {
        TypeRef { dims: 1, ..TypeRef::simple(binary) }
    }

    /// Spring's `RestClient.UriSpec` in miniature, all five `uri` overloads, plus a `with` pair that
    /// differs only in how many parameters its function takes.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        for class in ["java/lang/Object", "java/lang/String", "java/net/URI"] {
            members.insert(class.to_string(), cm(vec![]));
        }
        members.insert(
            "java/util/Map".into(),
            interface(vec![
                abstract_meth("size", "int", &[]),
                abstract_meth("get", "java/lang/Object", &["java/lang/Object"]),
            ]),
        );
        members.insert(
            "java/util/function/Function".into(),
            interface(vec![abstract_meth("apply", "R", &["T"])]),
        );
        members.insert(
            "java/util/function/BiFunction".into(),
            interface(vec![abstract_meth("apply", "R", &["T", "U"])]),
        );
        members.insert(
            "acme/UriBuilder".into(),
            interface(vec![
                abstract_meth("path", "acme/UriBuilder", &["java/lang/String"]),
                abstract_meth("build", "java/net/URI", &[]),
            ]),
        );
        let mut varargs = meth("uri", "acme/UriSpec", &["java/lang/String"]);
        varargs.params.push(array_of("java/lang/Object"));
        members.insert(
            "acme/UriSpec".into(),
            interface(vec![
                abstract_meth("uri", "acme/UriSpec", &["java/net/URI"]),
                varargs,
                abstract_meth("uri", "acme/UriSpec", &["java/lang/String", "java/util/Map"]),
                abstract_meth(
                    "uri",
                    "acme/UriSpec",
                    &["java/lang/String", "java/util/function/Function"],
                ),
                abstract_meth("uri", "acme/UriSpec", &["java/util/function/Function"]),
                abstract_meth("with", "acme/UriSpec", &["java/util/function/Function"]),
                abstract_meth("with", "acme/UriSpec", &["java/util/function/BiFunction"]),
                abstract_meth("pick", "acme/UriSpec", &["java/lang/Object"]),
                abstract_meth("pick", "acme/UriSpec", &["java/lang/String"]),
            ]),
        );
        members.insert(
            "acme/Client".into(),
            cm(vec![meth("post", "acme/UriSpec", &[])]),
        );
        let simple = [
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
            ("URI", "java/net/URI"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// The parameter types of the overload the call at the LAST `.name(` in `body` binds to.
    fn chosen(body: &str, name: &str) -> Option<Vec<String>> {
        let src = format!(
            "class C {{ URI build(acme.UriBuilder b) {{ return null; }}\n\
             void m(acme.Client client, URI someUri, Object a, Object b) {{\n{body}\n}} }}"
        );
        let needle = format!(".{name}(");
        let at = src.rfind(&needle).expect("call present") + 1;
        call_overload_at(&src, at, &resolver()).map(|m| {
            m.params
                .iter()
                .map(|p| format!("{}{}", p.binary_name, "[]".repeat(p.dims as usize)))
                .collect()
        })
    }

    fn params(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_lambda_binds_to_the_function_overload_not_the_first_declared() {
        let body = "client.post()\n        .uri(uri_builder ->\n            \
                    uri_builder.path(\"/v1/public/authorization/users-with-delegate\")\n\n        );";
        assert_eq!(chosen(body, "uri"), params(&["java/util/function/Function"]));
    }

    #[test]
    fn a_uri_typed_argument_binds_to_the_uri_overload() {
        assert_eq!(chosen("client.post().uri(someUri);", "uri"), params(&["java/net/URI"]));
    }

    #[test]
    fn extra_arguments_bind_to_the_varargs_overload() {
        assert_eq!(
            chosen("client.post().uri(\"x\", a, b);", "uri"),
            params(&["java/lang/String", "java/lang/Object[]"])
        );
    }

    #[test]
    fn a_template_and_a_lambda_bind_to_the_string_function_overload() {
        // The `Map` and `Object...` overloads take two arguments too; neither takes a lambda.
        assert_eq!(
            chosen("client.post().uri(\"x\", u -> u.build());", "uri"),
            params(&["java/lang/String", "java/util/function/Function"])
        );
    }

    #[test]
    fn a_method_reference_binds_to_the_function_overload() {
        assert_eq!(
            chosen("client.post().uri(this::build);", "uri"),
            params(&["java/util/function/Function"])
        );
    }

    #[test]
    fn a_lambda_whose_body_is_still_being_typed_still_binds() {
        assert_eq!(
            chosen("client.post().uri(u -> u.path(\"/x\").);", "uri"),
            params(&["java/util/function/Function"])
        );
    }

    #[test]
    fn the_lambda_parameter_count_picks_between_function_shapes() {
        assert_eq!(
            chosen("client.post().with((x, y) -> x);", "with"),
            params(&["java/util/function/BiFunction"])
        );
        assert_eq!(
            chosen("client.post().with(x -> x);", "with"),
            params(&["java/util/function/Function"])
        );
    }

    #[test]
    fn null_binds_to_the_most_specific_reference_overload() {
        assert_eq!(chosen("client.post().pick(null);", "pick"), params(&["java/lang/String"]));
    }

    /// An argument nothing can type excludes nothing and decides nothing: `pick(Object)` and
    /// `pick(String)` both stand, so there is no answer to give.
    #[test]
    fn an_untyped_argument_leaves_the_choice_open() {
        assert_eq!(chosen("client.post().pick(mystery());", "pick"), None);
    }

    #[test]
    fn primitive_widening_is_ordered() {
        assert!(widens("int", "long"));
        assert!(widens("char", "int"));
        assert!(widens("int", "int"));
        assert!(!widens("long", "int"));
        assert!(!widens("boolean", "int"));
        assert!(!widens("char", "short"));
    }
}
