//! Shared supertype walk for the resolver-backed checks. Every "does this type (or an ancestor)
//! declare X?" check — unknown method, unknown field, arity — walks the same class → superclass →
//! interfaces graph with the same **conservative** rule, so it lives here once.
//!
//! The rule (docs: never a false "cannot resolve"): an unknown class in the walk (`members_of`
//! returns `None`) short-circuits to `true` — an un-indexed base class might declare the member, so
//! we never wrongly report it absent. A `false` is returned only when the *entire* reachable
//! hierarchy is known and nothing matched.

use std::collections::HashSet;

use bennu_java::prelude::{ClassMembers, TypeResolver};

/// Depth guard against a pathological / cyclic hierarchy (cycles are also caught by `visited`).
const MAX_DEPTH: usize = 40;

/// Whether `binary` or any supertype satisfies `matches`, treating an unknown class as a match
/// (conservative). See the module docs.
pub fn hierarchy_has(
    resolver: &dyn TypeResolver,
    binary: &str,
    matches: &dyn Fn(&ClassMembers) -> bool,
) -> bool {
    let mut visited = HashSet::new();
    go(resolver, binary, matches, &mut visited, 0)
}

fn go(
    resolver: &dyn TypeResolver,
    binary: &str,
    matches: &dyn Fn(&ClassMembers) -> bool,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    if depth > MAX_DEPTH || !visited.insert(binary.to_string()) {
        return false;
    }
    let Some(cm) = resolver.members_of(binary) else {
        return true; // unknown type → can't rule the member out
    };
    if matches(&cm) {
        return true;
    }
    if let Some(sc) = &cm.superclass {
        if go(resolver, sc, matches, visited, depth + 1) {
            return true;
        }
    }
    for iface in &cm.interfaces {
        if go(resolver, iface, matches, visited, depth + 1) {
            return true;
        }
    }
    false
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
    let mut visited = HashSet::new();
    let mut stack = vec![binary.to_string()];
    let mut depth = 0usize;
    while let Some(bn) = stack.pop() {
        if depth > MAX_DEPTH * 4 || !visited.insert(bn.clone()) {
            continue;
        }
        depth += 1;
        if let Some(cm) = resolver.members_of(&bn) {
            visit(&bn, &cm);
            if let Some(sc) = &cm.superclass {
                stack.push(sc.clone());
            }
            stack.extend(cm.interfaces.iter().cloned());
        }
    }
}

/// Whether `target` is `from` itself or a supertype of `from` (i.e. a value of `from` is-a `target`).
/// Conservative: an unknown class in the walk short-circuits to `true` (it *might* be / lead to
/// `target`), so a positive "unrelated" conclusion is only drawn over a fully-known hierarchy.
pub fn reaches(resolver: &dyn TypeResolver, from: &str, target: &str) -> bool {
    let mut visited = HashSet::new();
    reaches_go(resolver, from, target, &mut visited, 0)
}

fn reaches_go(
    resolver: &dyn TypeResolver,
    from: &str,
    target: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    // A nested type has two binary spellings and both are in circulation — see
    // `bennu_java::prelude::same_binary_type`.
    if bennu_java::prelude::same_binary_type(from, target) {
        return true;
    }
    if depth > MAX_DEPTH || !visited.insert(from.to_string()) {
        return false;
    }
    let Some(cm) = resolver.members_of(from) else {
        return true; // unknown → can't rule out the relation
    };
    if let Some(sc) = &cm.superclass {
        if reaches_go(resolver, sc, target, visited, depth + 1) {
            return true;
        }
    }
    cm.interfaces.iter().any(|i| reaches_go(resolver, i, target, visited, depth + 1))
}

/// Whether every class in `binary`'s hierarchy is resolvable (no `members_of` gap). Checks that want
/// to make a *positive* assertion ("this class fails to implement X") need this: with an unknown
/// supertype the assertion could be wrong, so they bail. `false` also when `binary` itself is unknown.
pub fn hierarchy_fully_known(resolver: &dyn TypeResolver, binary: &str) -> bool {
    let mut visited = HashSet::new();
    known(resolver, binary, &mut visited, 0)
}

fn known(
    resolver: &dyn TypeResolver,
    binary: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    if !visited.insert(binary.to_string()) {
        return true; // already validated on another path
    }
    let Some(cm) = resolver.members_of(binary) else {
        return false;
    };
    if let Some(sc) = &cm.superclass {
        if !known(resolver, sc, visited, depth + 1) {
            return false;
        }
    }
    cm.interfaces.iter().all(|i| known(resolver, i, visited, depth + 1))
}
