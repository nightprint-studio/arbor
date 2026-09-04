//! The **conservative reading** of a supertype walk, for the resolver-backed checks.
//!
//! The traversal itself is [`bennu_java::prelude::walk`] — one graph walk for the whole engine, one
//! cycle guard, one node budget. What lives here is the policy the checks need on top of it, which
//! nothing else wants: an incomplete hierarchy answers **yes**.
//!
//! The rule (docs: never a false "cannot resolve"): a class the resolver cannot answer for might be
//! the one that declares the member, so we never report it absent on a hierarchy we could not read
//! to the end. A `false` is returned only when the *entire* reachable hierarchy is known and
//! nothing matched — which is exactly what [`bennu_java::prelude::Walk::complete`] says.

use bennu_java::prelude::{walk, walk_up, ClassMembers, TypeRef, TypeResolver};

/// Whether `binary` or any supertype satisfies `matches`, treating an unknown class as a match
/// (conservative). See the module docs.
pub fn hierarchy_has(
    resolver: &dyn TypeResolver,
    binary: &str,
    matches: &dyn Fn(&ClassMembers) -> bool,
) -> bool {
    let walked = walk(resolver, &TypeRef::simple(binary), |a| matches(&a.members).then_some(()));
    walked.found.is_some() || !walked.complete
}

/// Visit `binary` + every KNOWN supertype, calling `visit(members)` on each. Unlike
/// [`hierarchy_has`] this doesn't short-circuit — it's for checks that must aggregate across the
/// whole hierarchy (e.g. collecting every abstract method). An unknown class is simply skipped;
/// the caller decides what an incomplete hierarchy means.
pub fn for_each_supertype(
    resolver: &dyn TypeResolver,
    binary: &str,
    visit: &mut dyn FnMut(&str, &ClassMembers),
) {
    walk_up::<()>(resolver, &TypeRef::simple(binary), |a| {
        visit(&a.ty.binary_name, &a.members);
        None
    });
}

/// Whether `target` is `from` itself or a supertype of `from` (i.e. a value of `from` is-a `target`).
/// Conservative: an unknown class in the walk short-circuits to `true` (it *might* be / lead to
/// `target`), so a positive "unrelated" conclusion is only drawn over a fully-known hierarchy.
pub fn reaches(resolver: &dyn TypeResolver, from: &str, target: &str) -> bool {
    // A nested type has two binary spellings and both are in circulation — see
    // `bennu_java::prelude::same_binary_type`.
    let walked = walk(resolver, &TypeRef::simple(from), |a| {
        bennu_java::prelude::same_binary_type(&a.ty.binary_name, target).then_some(())
    });
    walked.found.is_some() || !walked.complete
}

/// Whether every class in `binary`'s hierarchy is resolvable (no `members_of` gap). Checks that want
/// to make a *positive* assertion ("this class fails to implement X") need this: with an unknown
/// supertype the assertion could be wrong, so they bail. `false` also when `binary` itself is unknown.
///
/// Which is the shared walk's own verdict — this is the question `complete` was added to answer,
/// and asking it a second way was how two walks came to disagree about the same hierarchy.
pub fn hierarchy_fully_known(resolver: &dyn TypeResolver, binary: &str) -> bool {
    walk::<()>(resolver, &TypeRef::simple(binary), |_| None).complete
}
