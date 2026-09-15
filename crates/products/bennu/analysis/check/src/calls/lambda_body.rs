//! Whether a lambda's BODY, or a method reference, fits the functional interface it is written
//! against — the half of lambda typing the arity check does not reach.
//!
//! A lambda with the right number of parameters can still be the wrong lambda:
//!
//!   * `Runnable r = () -> 42;` — a `void` method's lambda must be a *statement*, and `42` is a value;
//!   * `Runnable r = () -> { return 1; };` — nor may its block return one;
//!   * `Supplier<String> s = () -> 1;` — a value lambda must produce what the method returns.
//!
//! A method reference has the same questions in another spelling (JLS §15.13.1): does a method of the
//! name take what the interface passes — as a static method, as an instance method bound to a
//! receiver, or unbound with the first argument as its receiver — is exactly one of those forms
//! applicable, and does what it returns fit?
//!
//! Everything is read through the interface's type arguments (`Function<String, Integer>.apply`
//! returns `Integer`). Wherever an answer would need a type the file does not pin down — a type
//! variable, a captured wildcard, an unreadable class, a lambda parameter inference could not type —
//! the question is not asked.

use bennu_java::prelude::{
    infer_node_type_cached, single_abstract_method, substitute, walk, FileSymbols, InferCache, Member,
    MemberKind, TypeRef, TypeResolver,
};
use tree_sitter::Node;

use crate::flow::returns::{block_can_complete_normally, collect_returns};
use crate::support::assignable::{definite_mismatch, names_a_declared_object, null_mismatch, spells_its_type};
use crate::support::nodes::{is_primitive, simple_name};
use crate::support::resolve::{static_receiver_binary, type_binary_at};

/// What a judgement reads about the file.
pub(crate) struct Ctx<'a, 't> {
    pub(crate) root: &'a Node<'t>,
    pub(crate) source: &'a str,
    pub(crate) symbols: &'a FileSymbols,
    pub(crate) resolver: &'a dyn TypeResolver,
    pub(crate) cache: &'a InferCache,
}

/// A functional interface's single abstract method, read through the target's type arguments.
pub(crate) struct Sam {
    /// `Function.apply` — for messages.
    pub(crate) label: String,
    pub(crate) params: Vec<TypeRef>,
    pub(crate) returns: TypeRef,
}

impl Sam {
    fn is_void(&self) -> bool {
        is_void(&self.returns)
    }
}

/// The SAM of `target`, its parameters and return type substituted with the arguments `target`
/// gives the interface that declares the method. `None` for anything that is not a readable
/// functional interface.
pub(crate) fn sam_through(target: &TypeRef, resolver: &dyn TypeResolver) -> Option<Sam> {
    if target.dims > 0 || target.names_a_wildcard() || !resolver.members_of(&target.binary_name)?.flags.is_interface {
        return None;
    }
    let sam = single_abstract_method(resolver, &TypeRef::simple(target.binary_name.clone()))?;
    let declaring = walk(resolver, target, |a| {
        a.members
            .methods
            .iter()
            .any(|m| m.kind == MemberKind::Method && m.name == sam.name && m.params.len() == sam.params.len())
            .then(|| a.ty.clone())
    })
    .found?;
    let type_params = resolver.members_of(&declaring.binary_name)?.type_params.clone();
    let read = |t: &TypeRef| substitute(t, &type_params, &declaring.type_args);
    Some(Sam {
        label: format!("{}.{}", simple_name(&target.binary_name), sam.name),
        params: sam.params.iter().map(read).collect(),
        returns: read(&sam.return_type),
    })
}

/// Why `lambda`'s body cannot implement `sam`, when it provably cannot.
pub(crate) fn lambda_body_mismatch(lambda: Node, sam: &Sam, ctx: &Ctx) -> Option<String> {
    let body = lambda.child_by_field_name("body")?;
    if body.has_error() {
        return None;
    }
    if body.kind() != "block" {
        if sam.is_void() {
            // JLS §15.27.3: an expression body of a `void` lambda must be a statement expression.
            let statement = matches!(
                body.kind(),
                "method_invocation" | "assignment_expression" | "update_expression" | "object_creation_expression"
            );
            return (!statement).then(|| format!("`{}` returns nothing, and this lambda's body is a value", sam.label));
        }
        return value_mismatch(body, sam, ctx);
    }
    let mut returns = Vec::new();
    collect_returns(body, &mut returns);
    let values: Vec<Node> = returns.iter().filter_map(|r| returned_value(*r)).collect();
    let completes = block_can_complete_normally(body, ctx.source.as_bytes());
    if !values.is_empty() && completes {
        return None; // neither value- nor void-compatible: `returns::lambda_body_errors_nodes` says so
    }
    if sam.is_void() {
        return (!values.is_empty()).then(|| format!("`{}` returns nothing, and this lambda returns a value", sam.label));
    }
    if values.is_empty() {
        if !completes {
            return None; // a body that only throws fits any return type
        }
        return Some(format!("`{}` returns `{}`, and this lambda returns nothing", sam.label, render(&sam.returns)));
    }
    values.into_iter().find_map(|v| value_mismatch(v, sam, ctx))
}

/// Why the value `expr` cannot be what `sam` returns.
fn value_mismatch(expr: Node, sam: &Sam, ctx: &Ctx) -> Option<String> {
    if !concrete(&sam.returns, ctx.resolver) {
        return None;
    }
    let refused = |found: &str| format!("`{}` returns `{}`, and this lambda returns `{found}`", sam.label, render(&sam.returns));
    if expr.kind() == "null_literal" {
        return null_mismatch(&sam.returns).map(|_| refused("null"));
    }
    let ty = infer_node_type_cached(ctx.root, ctx.source, ctx.symbols, &expr, ctx.resolver, ctx.cache)?;
    if is_void(&ty) {
        return Some(format!("`{}` returns `{}`, and this lambda's body returns nothing", sam.label, render(&sam.returns)));
    }
    let written = spells_its_type(expr) || names_a_declared_object(expr, ctx.source.as_bytes());
    definite_mismatch(&ty, &sam.returns, written, ctx.resolver).map(|m| refused(&m.found))
}

/// Why the method reference `reference` cannot implement `sam`, when it provably cannot.
pub(crate) fn method_reference_mismatch(reference: Node, sam: &Sam, ctx: &Ctx) -> Option<String> {
    if reference.has_error() {
        return None;
    }
    let bytes = ctx.source.as_bytes();
    let mut c = reference.walk();
    let children: Vec<Node> = reference.named_children(&mut c).collect();
    let (qualifier, name) = (*children.first()?, *children.last()?);
    if qualifier.id() == name.id() {
        let is_constructor = reference.utf8_text(bytes).ok()?.trim_end().ends_with("new");
        return if is_constructor { constructor_mismatch(qualifier, sam, ctx) } else { None };
    }
    if matches!(qualifier.kind(), "super" | "this") || name.kind() != "identifier" {
        return None;
    }
    let method = name.utf8_text(bytes).ok()?;
    let written = format!("{}::{method}", qualifier.utf8_text(bytes).ok()?.trim());
    // A name that reads as a type is a type (a variable of the same name would obscure it, and
    // `static_receiver_binary` refuses exactly those); anything else is a value the reference binds.
    let (receiver, bound) = match static_receiver_binary(qualifier, bytes, ctx.symbols, ctx.resolver) {
        Some(binary) => (TypeRef::simple(binary), false),
        None => (infer_node_type_cached(ctx.root, ctx.source, ctx.symbols, &qualifier, ctx.resolver, ctx.cache)?, true),
    };
    if receiver.dims > 0 || is_primitive(&receiver.binary_name) || !receiver.binary_name.contains('/') {
        return None;
    }
    let res = ctx.cache.resolve_methods(ctx.resolver, &receiver.binary_name, method);
    if !res.complete || res.candidates.is_empty() || res.candidates.iter().any(varargs) {
        return None;
    }
    let arity = sam.params.len();
    let of_arity = |is_static: bool, n: usize| -> Vec<&Member> {
        res.candidates.iter().filter(|m| m.is_static == is_static && m.params.len() == n).collect()
    };
    let statics = of_arity(true, arity);
    let instances = of_arity(false, arity);
    if bound {
        // `expr::m` — only an instance method takes the receiver the expression supplies.
        if instances.is_empty() {
            return Some(match statics.is_empty() {
                false => format!("`{written}` names a static method, which cannot be called through an instance"),
                true => format!("`{written}` cannot take the {arity} argument{} `{}` passes", plural(arity), sam.label),
            });
        }
        return single_return_mismatch(&instances, &written, sam, ctx);
    }
    // `Type::m` — a static method taking every argument, or an instance method taking all but the
    // first, which becomes its receiver.
    let unbound = if arity == 0 { Vec::new() } else { of_arity(false, arity - 1) };
    if statics.is_empty() && unbound.is_empty() {
        return Some(match instances.is_empty() {
            false => format!("`{written}` names an instance method, and `{}` supplies no receiver for it", sam.label),
            true => format!("`{written}` cannot take the {arity} argument{} `{}` passes", plural(arity), sam.label),
        });
    }
    if !sam.params.iter().all(|p| concrete(p, ctx.resolver)) {
        return None;
    }
    let static_fits = |strict: bool| -> Vec<&Member> {
        statics.iter().copied().filter(|m| accepts(m, &sam.params, strict, ctx)).collect()
    };
    // The unbound form needs a first argument to become the receiver: none when the SAM takes nothing.
    let unbound_fits = |strict: bool| -> Vec<&Member> {
        let Some((first, rest)) = sam.params.split_first() else { return Vec::new() };
        let receiver_fits = (!strict || concrete(&receiver, ctx.resolver))
            && definite_mismatch(first, &receiver, true, ctx.resolver).is_none();
        match receiver_fits {
            true => unbound.iter().copied().filter(|m| accepts(m, rest, strict, ctx)).collect(),
            false => Vec::new(),
        }
    };
    // JLS §15.13.1: both searches finding a method is an error. Asserted only when both are
    // applicable on types read in full — a lenient "not refused" would call a merely unreadable
    // parameter a second fit.
    if !static_fits(true).is_empty() && !unbound_fits(true).is_empty() {
        return Some(format!("`{written}` is ambiguous: a static and an instance `{method}` both fit `{}`", sam.label));
    }
    let (statics_lenient, unbound_lenient) = (static_fits(false), unbound_fits(false));
    match (statics_lenient.is_empty(), unbound_lenient.is_empty()) {
        (false, true) => single_return_mismatch(&statics_lenient, &written, sam, ctx),
        (true, false) => single_return_mismatch(&unbound_lenient, &written, sam, ctx),
        _ => None,
    }
}

/// `Type::new` — some constructor of `Type` has to take what the interface passes.
fn constructor_mismatch(qualifier: Node, sam: &Sam, ctx: &Ctx) -> Option<String> {
    let bytes = ctx.source.as_bytes();
    if !matches!(qualifier.kind(), "type_identifier" | "scoped_type_identifier" | "identifier" | "field_access" | "scoped_identifier") {
        return None;
    }
    let binary = type_binary_at(qualifier.utf8_text(bytes).ok()?, qualifier, bytes, ctx.symbols, ctx.resolver)?;
    // A compiled inner class's constructor takes its enclosing instance first; the source model's
    // does not, so only a project type's nested constructors are read as written.
    if binary.contains('$') && !ctx.resolver.is_project_type(&binary) {
        return None;
    }
    let members = ctx.resolver.members_of(&binary)?;
    let flags = &members.flags;
    if flags.is_abstract || flags.is_interface || flags.is_enum || flags.is_record || flags.has_hidden_members {
        return None;
    }
    let ctors: Vec<&Member> = members.methods.iter().filter(|m| m.name == "<init>").collect();
    if ctors.is_empty() || ctors.iter().any(|m| varargs(m)) || !sam.params.iter().all(|p| concrete(p, ctx.resolver)) {
        return None;
    }
    let arity = sam.params.len();
    let type_name = simple_name(&binary).rsplit('$').next().unwrap_or_default();
    if ctors.iter().all(|m| m.params.len() != arity) {
        return Some(format!("No constructor of `{type_name}` takes the {arity} argument{} `{}` passes", plural(arity), sam.label));
    }
    ctors.iter().filter(|m| m.params.len() == arity).all(|m| !accepts(m, &sam.params, false, ctx)).then(|| {
        let given: Vec<String> = sam.params.iter().map(render).collect();
        format!("No constructor of `{type_name}` accepts `{}`, which `{}` passes", given.join(", "), sam.label)
    })
}

/// When exactly one method fits, whether what it returns can be what `sam` returns.
fn single_return_mismatch(fits: &[&Member], written: &str, sam: &Sam, ctx: &Ctx) -> Option<String> {
    // The hierarchy walk keeps an override beside what it overrides (`String.length` and
    // `CharSequence.length`): one method, nearest first — and the nearest has the covariant return.
    let mut distinct: Vec<&Member> = Vec::new();
    for m in fits {
        if !distinct.iter().any(|d| same_erased_params(d, m)) {
            distinct.push(m);
        }
    }
    let [only] = distinct.as_slice() else { return None };
    if sam.is_void() || !concrete(&sam.returns, ctx.resolver) {
        return None; // a value-returning method is fine where nothing is expected (JLS §15.13.2)
    }
    if is_void(&only.return_type) {
        return Some(format!("`{written}` returns nothing, and `{}` returns `{}`", sam.label, render(&sam.returns)));
    }
    if !concrete(&only.return_type, ctx.resolver) {
        return None;
    }
    definite_mismatch(&only.return_type, &sam.returns, false, ctx.resolver)
        .map(|m| format!("`{written}` returns `{}`, and `{}` returns `{}`", m.found, sam.label, render(&sam.returns)))
}

/// Whether `m` takes the argument types `given`, position by position. Lenient: `false` only on a
/// definite refusal somewhere. Strict: also `false` wherever a parameter cannot be read in full.
fn accepts(m: &Member, given: &[TypeRef], strict: bool, ctx: &Ctx) -> bool {
    m.params.len() == given.len()
        && m.params.iter().zip(given).all(|(param, arg)| match concrete(param, ctx.resolver) {
            // The interface's parameter types are DECLARED ones, so an `Object` among them is
            // really `Object` — never an erased type variable.
            true => definite_mismatch(arg, param, true, ctx.resolver).is_none(),
            false => !strict,
        })
}

/// A type every question here can be asked of: a primitive or a readable class, never a type
/// variable or a captured wildcard, at any depth of its arguments.
fn concrete(t: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    if t.names_a_wildcard() {
        return false;
    }
    let name = t.binary_name.as_str();
    if is_primitive(name) {
        return true;
    }
    name.contains('/') && resolver.members_of(name).is_some() && t.type_args.iter().all(|a| concrete(a, resolver))
}

fn same_erased_params(a: &Member, b: &Member) -> bool {
    a.params.len() == b.params.len()
        && a.params.iter().zip(&b.params).all(|(x, y)| x.binary_name == y.binary_name && x.dims == y.dims)
}

fn is_void(t: &TypeRef) -> bool {
    t.dims == 0 && t.binary_name == "void"
}

fn varargs(m: &Member) -> bool {
    m.params.last().is_some_and(TypeRef::is_array)
}

/// The value a `return` carries, if any.
fn returned_value(ret: Node) -> Option<Node> {
    let mut c = ret.walk();
    let value = ret.named_children(&mut c).find(|n| !matches!(n.kind(), "line_comment" | "block_comment"));
    value
}

fn render(t: &TypeRef) -> String {
    let name = t.binary_name.as_str();
    t.with_brackets(if is_primitive(name) || name == "void" { name } else { simple_name(name) })
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
