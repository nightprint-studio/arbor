//! What a single expression can throw — the one reading of a `throws` clause.
//!
//! Two checks need it and they want opposite answers from it. [`crate::checked_call`] asks "is any of
//! this NOT handled" and reports what escapes; [`crate::dead_catch`] asks "could this ever throw E"
//! and reports a `catch` for something that cannot arrive. Written twice, the two would drift, and a
//! drift here is a false positive in one of them — the second-worst kind of bug in a validator, after
//! the crash.
//!
//! ## Why the answer is a three-state, not a list
//!
//! "This call throws nothing" and "I could not work out what this call throws" are the same empty
//! list and completely different facts. `checked_call` may treat them alike — it stays silent either
//! way — but `dead_catch` must not: it concludes something is NEVER thrown, and one unreadable call
//! in the body destroys that conclusion. Hence [`Thrown::Unknown`], which every SKIP returns.
//!
//! ## Overload soundness (carried over unchanged)
//!
//! A call may bind to any overload of that name, and full overload resolution is exactly the
//! machinery that produces false positives when done imperfectly. So the answer is the
//! **intersection** of the `throws` lists over every candidate: an exception every candidate declares
//! is thrown whichever one binds. One declaring nothing collapses the intersection, which is correct
//! — the compiler might be picking that one.

use bennu_java::prelude::{
    infer_node_type_cached, FileSymbols, InferCache, MemberKind, TypeResolver,
};
use tree_sitter::Node;

use crate::resolve::type_binary;

/// The two bounds on what a call throws — and they are genuinely two answers, not one rounded
/// differently.
///
/// A call may bind to any overload of that name, and we do not do full overload resolution. So:
///
///   * [`Self::definitely`] is the INTERSECTION over the candidates — thrown whichever one binds.
///     A lower bound, and the sound basis for "this must be caught or declared".
///   * [`Self::possibly`] is the UNION — thrown if the right one binds. An upper bound, and the
///     only sound basis for concluding something can NEVER arrive.
///
/// Using the first where the second belongs is not a rounding error, it is a wrong answer:
/// `Future.get(timeout, unit)` declares `TimeoutException` and `Future.get()` does not, so the
/// intersection drops it — and a check reading that as "never thrown" reports a `catch` guava has
/// needed since 2011.
pub(crate) struct Throws {
    /// Declared by EVERY candidate.
    pub(crate) definitely: Vec<String>,
    /// Declared by ANY candidate.
    pub(crate) possibly: Vec<String>,
}

impl Throws {
    /// Nothing is thrown — a real answer, distinct from [`Thrown::Unknown`].
    fn none() -> Self {
        Throws { definitely: Vec::new(), possibly: Vec::new() }
    }
}

/// What an expression throws, as far as the index can prove.
pub(crate) enum Thrown<'t> {
    /// Both bounds, and the node a diagnostic should be anchored on.
    Known(Node<'t>, Throws),
    /// Nothing could be determined: an unresolved receiver, a hierarchy with a gap, a constructor
    /// the index does not carry. A caller reasoning about what CANNOT happen has to give up here.
    Unknown,
}

/// What `n` throws, for the two node kinds that can throw by calling something.
pub(crate) fn thrown_by<'t>(
    n: Node<'t>,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Thrown<'t> {
    match n.kind() {
        "method_invocation" => thrown_by_invocation(n, root, source, bytes, symbols, resolver, cache),
        "object_creation_expression" => thrown_by_creation(n, bytes, symbols, resolver),
        _ => Thrown::Known(n, Throws::none()),
    }
}

#[allow(clippy::too_many_arguments)]
fn thrown_by_invocation<'t>(
    n: Node<'t>,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Thrown<'t> {
    let Some(name) = n.child_by_field_name("name") else { return Thrown::Unknown };
    if name.has_error() {
        return Thrown::Unknown;
    }
    let Ok(method) = name.utf8_text(bytes) else { return Thrown::Unknown };

    // SKIP: only an explicit-receiver call `obj.method(...)`. A bare `foo()` / implicit-`this` call
    // resolves against the enclosing type, whose source form we may not carry `throws` for reliably;
    // inferring it risks a false positive, so we stay silent (aligns with members/arity, which also
    // require an `object` field).
    let Some(obj) = n.child_by_field_name("object") else { return Thrown::Unknown };
    // SKIP: receiver type not inferable, or inferred to the empty/unknown type → we can't gather a
    // trustworthy candidate set (an un-indexed type might declare/overload the method differently).
    let Some(ty) = infer_node_type_cached(root, source, symbols, &obj, resolver, cache) else {
        return Thrown::Unknown;
    };
    if ty.binary_name.is_empty() {
        return Thrown::Unknown;
    }
    // The overload set for this name across the receiver's hierarchy (memoized walk shared with the
    // member/arity/argument checks). `complete` is the hierarchy-fully-known gate.
    let res = cache.resolve_methods(resolver, &ty.binary_name, method);
    // SKIP: an unknown supertype might carry an overload with a DIFFERENT (smaller) `throws` list,
    // which would shrink the true intersection — so a hidden overload could make our intersection an
    // over-estimate → a false positive. Only a fully-known hierarchy makes the intersection sound.
    if !res.complete {
        return Thrown::Unknown;
    }
    // SKIP: no candidate of that name (a missing method is `members.rs`'s job; here nothing definite
    // is thrown). Intersection over an empty set is meaningless → SKIP.
    if res.candidates.is_empty() {
        return Thrown::Unknown;
    }

    let thrown = bounds(&res.candidates.iter().collect::<Vec<_>>());
    // `x.clone()` reaching `Object.clone()` is not evidence of a checked exception.
    //
    // `Object.clone()` is `protected`, so a call on a plain receiver only compiles when the receiver
    // is an ARRAY — every array type overrides it public, covariant and `throws`-free (JLS §10.7),
    // and an array has no `ClassMembers` for the walk to find — or when some class overrode it with
    // its own `throws`, which the intersection would then carry. A class calling its own inherited
    // one writes `super.clone()`, a different receiver. So the shape below is the false positive and
    // nothing else, and it fired on `array.clone()` — which every Java program writes.
    let only_object_clone = method == "clone"
        && n.child_by_field_name("arguments").map(|a| a.named_child_count() == 0).unwrap_or(true)
        && thrown.possibly.len() == 1
        && thrown.possibly.iter().any(|t| t == "java/lang/CloneNotSupportedException");
    if only_object_clone {
        // A SUPPRESSION of the lower bound, not a fact about the upper one. `array.clone()` must not
        // be reported as an unhandled exception — every Java program writes it — but a receiver whose
        // own `clone()` really does declare `CloneNotSupportedException` (`javax.crypto.Mac` does)
        // can still throw it, and a check concluding "never" from a zeroed `possibly` reports a
        // `catch` the JDK's own contract requires.
        return Thrown::Known(name, Throws { definitely: Vec::new(), possibly: thrown.possibly });
    }
    Thrown::Known(name, thrown)
}

/// A `new T(args)` construction. Resolves `T`, gathers its OWN `<init>` members (constructors are not
/// inherited — mirror `arity::check_new`), intersects their `throws`, and flags unhandled checked
/// exceptions anchored on the `new`'s type node.
fn thrown_by_creation<'t>(
    n: Node<'t>,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Thrown<'t> {
    let Some(ty_node) = n.child_by_field_name("type") else { return Thrown::Unknown };

    // SKIP: an anonymous class `new Runnable(){…}` — the args bind to the supertype's constructor and
    // the anonymous body's own methods complicate the contract; stay out of it (mirror arity/members).
    // GOTCHA: explicit `for` loop over children (never `.any()` on `named_children`).
    let mut cw = n.walk();
    for c in n.named_children(&mut cw) {
        if c.kind() == "class_body" {
            return Thrown::Unknown;
        }
    }

    let Ok(type_text) = ty_node.utf8_text(bytes) else { return Thrown::Unknown };
    // SKIP: `T` unresolvable → we don't know which constructors exist.
    let Some(binary) = type_binary(type_text, symbols, resolver) else { return Thrown::Unknown };

    // Constructors are NOT inherited — look only at this class's own `<init>` methods (mirror arity).
    let Some(cm) = resolver.members_of(&binary) else { return Thrown::Unknown };
    let ctors: Vec<&bennu_java::prelude::Member> = {
        let mut v = Vec::new();
        for m in &cm.methods {
            if m.name == "<init>" && m.kind == MemberKind::Method {
                v.push(m);
            }
        }
        v
    };
    // SKIP: no constructors indexed (the index may omit them) → nothing definite → SKIP.
    if ctors.is_empty() {
        return Thrown::Unknown;
    }

    let thrown = bounds(&ctors);
    // Anchor the diagnostic on the `new`'s type node (there's no `name` field on a construction).
    Thrown::Known(ty_node, thrown)
}


/// Both bounds over the candidate overloads.
///
/// A single candidate declaring nothing collapses `definitely` to empty — correct, the compiler
/// might be picking that one — while `possibly` keeps everything any of them declares.
fn bounds(candidates: &[&bennu_java::prelude::Member]) -> Throws {
    let Some((first, rest)) = candidates.split_first() else { return Throws::none() };
    let mut definitely: Vec<String> = first.throws.clone();
    let mut possibly: Vec<String> = first.throws.clone();
    for cand in rest {
        definitely.retain(|x| cand.throws.iter().any(|y| y == x));
        for t in &cand.throws {
            if !possibly.iter().any(|y| y == t) {
                possibly.push(t.clone());
            }
        }
    }
    Throws { definitely, possibly }
}
