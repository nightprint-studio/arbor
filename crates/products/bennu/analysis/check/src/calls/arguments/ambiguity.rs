//! An **ambiguous** call — several overloads applicable and none more specific than the others
//! (JLS §15.12.2.5), which javac rejects as `ref.ambiguous`.
//!
//! `bennu_java`'s rules keep several candidates for two reasons, and only one of them is an error:
//! the call really is ambiguous, or the rules could not compare the candidates — an argument nothing
//! typed, a type variable (which fits in every phase), a hierarchy the classpath cannot read. A
//! report therefore needs the second reason ruled out on this module's own evidence:
//!   * every argument typed, or `null` — never a lambda, a method reference, an untyped expression;
//!   * every parameter of every survivor a primitive or a type the classpath reads, never a type
//!     variable;
//!   * the survivors all of one arity, so no varargs expansion compares a prefix against an element;
//!   * wherever two survivors' parameters differ, the subtype question decided both ways.

use bennu_java::prelude::{subtype_verdict, Member, TypeRef, TypeResolver};

use super::Judged;
use crate::support::nodes::{is_primitive, is_type_var, simple_name};
use crate::support::walk::hierarchy_fully_known;

/// Whether `kept` — the overloads the applicability rules left standing — are provably ambiguous
/// for a call passing `args`.
pub(super) fn provably_ambiguous(kept: &[&Member], args: &[Judged], resolver: &dyn TypeResolver) -> bool {
    let [first, rest @ ..] = kept else { return false };
    if rest.is_empty() || rest.iter().any(|m| m.params.len() != first.params.len()) {
        return false;
    }
    if !args.iter().all(|a| matches!(a, Judged::Typed { .. } | Judged::Null) || is_implicit_lambda(a)) {
        return false;
    }
    if !kept.iter().flat_map(|m| &m.params).all(|p| readable(p, resolver)) {
        return false;
    }
    // A candidate the caller may not be allowed to call is not a candidate: `new URLClassLoader(urls,
    // null)` has a package-private `(URL[], AccessControlContext)` beside the public `(URL[],
    // ClassLoader)`, and `new ExecutionException(null)` a protected `(String)` beside `(Throwable)`.
    // Whether it is visible from here is the visibility check's question; this one stays out of it.
    // Candidates of one visibility stand or fall together, so only a MIXED set is in doubt.
    if kept.iter().any(|m| m.visibility != first.visibility) {
        return false;
    }
    // An argument whose own hierarchy has a gap may satisfy a parameter the walk could not see it
    // satisfy — or fail one it seemed to — so "both match" is not established. (Guava's
    // `TrustedListenableFutureTask`, a `Runnable`, was ambiguous with `Callable` through one.)
    let known = |a: &Judged| match a {
        Judged::Typed { ty, .. } => ty.dims > 0 || is_primitive(&ty.binary_name) || hierarchy_fully_known(resolver, &ty.binary_name),
        _ => true,
    };
    if !args.iter().all(known) {
        return false;
    }
    // Two survivors with the same erased parameters are ONE method met twice — an override, or a
    // declaration an interface and a class both carry, each spelling its type arguments its own way
    // (`sort(Comparator<? super E>)` beside `sort(Comparator<? super String>)`). Never an ambiguity.
    let erased = |m: &Member| -> Vec<(String, u8)> { m.params.iter().map(|p| (p.binary_name.clone(), p.dims)).collect() };
    for (i, a) in kept.iter().enumerate() {
        if kept[i + 1..].iter().any(|b| erased(a) == erased(b)) {
            return false;
        }
    }
    kept.iter().all(|a| {
        kept.iter().all(|b| {
            a.params.iter().zip(&b.params).zip(args).all(|((pa, pb), arg)| {
                pa == pb
                    || match is_implicit_lambda(arg) {
                        // JLS §15.12.2.5: an implicitly typed lambda gives no basis for preferring one
                        // functional interface over another, so two that are not related by subtyping
                        // leave the call ambiguous — and related ones are for the rules to settle.
                        true => unrelated(pa, pb, resolver),
                        false => decided(pa, pb, resolver),
                    }
            })
        })
    })
}

/// A lambda whose parameters are written without types (`s -> …`, `(a, b) -> …`) and whose body fits
/// both a `void` and a value-returning interface — an expression that is also a statement (a call, an
/// assignment, `++`, `new`), as in `s -> s.length()`. A block, or an expression that is no statement
/// (`s -> s + 1`), is compatible with only one of the two, and which one is the rules' to decide.
///
/// `() -> …` has no parameter to leave untyped, and counts as explicitly typed.
fn is_implicit_lambda(arg: &Judged) -> bool {
    let Judged::Lambda(lambda) = arg else { return false };
    let untyped = lambda.child_by_field_name("parameters").is_some_and(|p| matches!(p.kind(), "identifier" | "inferred_parameters"));
    let fits_either = lambda.child_by_field_name("body").is_some_and(|body| {
        matches!(body.kind(), "method_invocation" | "assignment_expression" | "update_expression" | "object_creation_expression")
    });
    untyped && fits_either
}

/// Two parameter types of which neither is a subtype of the other — both answers known and negative.
fn unrelated(a: &TypeRef, b: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    subtype_verdict(resolver, a, b) == Some(false) && subtype_verdict(resolver, b, a) == Some(false)
}

/// `Ambiguous call to `pick`: `pick(String)` and `pick(StringBuilder)` both match`.
pub(super) fn message(name: &str, kept: &[&Member]) -> String {
    let signatures: Vec<String> = kept
        .iter()
        .map(|m| {
            let params: Vec<String> = m.params.iter().map(render).collect();
            format!("`{name}({})`", params.join(", "))
        })
        .collect();
    let verdict = if kept.len() == 2 { "both match" } else { "all match" };
    format!("Ambiguous call to `{name}`: {} {verdict}", signatures.join(" and "))
}

/// A parameter type the specificity rules can reason about: a primitive, or a class the classpath
/// reads — never a type variable, whatever its name.
fn readable(p: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    let name = p.binary_name.as_str();
    if name.is_empty() || name.ends_with("[]") || is_type_var(name) {
        return false;
    }
    (is_primitive(name) && name != "void") || resolver.members_of(name).is_some()
}

/// Whether "is one more specific than the other" has a definite answer for two differing parameter
/// types. A primitive against anything is settled without the classpath (no subtyping crosses
/// between them), two references need the verdict both ways.
fn decided(a: &TypeRef, b: &TypeRef, resolver: &dyn TypeResolver) -> bool {
    let primitive = |t: &TypeRef| t.dims == 0 && is_primitive(&t.binary_name);
    if primitive(a) || primitive(b) {
        return true;
    }
    subtype_verdict(resolver, a, b).is_some() && subtype_verdict(resolver, b, a).is_some()
}

fn render(p: &TypeRef) -> String {
    let element = if is_primitive(&p.binary_name) { p.binary_name.as_str() } else { simple_name(&p.binary_name) };
    p.with_brackets(element)
}
