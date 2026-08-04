//! Shared helper: resolve a *written* type name (as it appears in source — `Foo`, `Foo<Bar>`,
//! `com.acme.Foo`) to a JVM binary name, using the file's imports + same-file declarations + the
//! resolver. Used by the constructor-arity and inheritance checks, so the resolution rules live once.

use std::collections::HashSet;

use bennu_java::prelude::{FileSymbols, TypeResolver};

/// Resolve `text` (a type as written) to a binary name (`com/acme/Foo`). Strips generic arguments.
/// `None` when unresolvable — callers treat that conservatively (skip).
///
/// Resolution order mirrors Java name lookup (JLS §6.5.5):
///   1. a fully-qualified name (`a.b.C`) → its slash form;
///   2. a type declared in THIS file (same compilation unit);
///   3. an explicit single-type import (`import a.b.C;`);
///   4. a type in the file's OWN package — in scope WITHOUT an import (JLS §6.3). Crucially preferred
///      over a same-simple-name type in another package: the resolver's flat simple-name index
///      collapses duplicate simple names, so a bare `C` referenced from `com.acme` would otherwise
///      resolve to an arbitrary `C` (whichever the index kept), producing false errors on legal
///      same-package code. We check `com/acme/C` directly (a unique binary key) instead.
///   5. otherwise the resolver's global lookup (wildcard imports, JDK, project simple-name index).
pub fn type_binary(text: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<String> {
    let simple = text.split('<').next().unwrap_or(text).trim();
    if simple.is_empty() {
        return None;
    }
    if simple.contains('.') {
        return Some(simple.replace('.', "/"));
    }
    // A type declared in THIS file is authoritative (its FQN is on the extracted symbols).
    if let Some(td) = symbols.types.iter().find(|t| t.name == simple) {
        return Some(td.fqn.replace('.', "/"));
    }
    // An explicit single-type import wins over a same-package type (JLS §7.5.1).
    for imp in &symbols.imports {
        if imp.simple_name() == Some(simple) {
            return Some(imp.path.replace('.', "/"));
        }
    }
    // A type in the file's OWN package — resolvable without an import. Look the exact binary up (a
    // unique key), so a same-package type is never shadowed by a same-simple-name type elsewhere.
    if let Some(candidate) = same_package_binary(simple, symbols) {
        if resolver.members_of(&candidate).is_some() {
            return Some(candidate);
        }
    }
    resolver.resolve_simple_name(simple, &symbols.imports)
}

/// A member type named `simple` **inherited** by `owner` (a binary name), searching its
/// supertype chain — or declared on `owner` itself.
///
/// This is the lookup step [`type_binary`] cannot do on its own, because it needs to know
/// which type the name was written *inside*. A nested type declared in a superclass or a
/// superinterface is in scope in the subclass **by its simple name, with no import**
/// (JLS §8.1.5): given `class Base { public static class Inner {} }`, a
/// `class Sub extends Base` writes `Inner` and means `Base.Inner`. Nobody imports it,
/// because there is nothing to import — and a checker that doesn't know this reports
/// perfectly good code as a broken build.
///
/// Returns the binary name that answered, so a caller can go on to look members up on it.
pub fn inherited_member_type(
    owner: &str,
    simple: &str,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    // An explicit stack: a hierarchy can be deep, and `seen` also makes a cyclic one
    // (which broken source can express) terminate rather than hang.
    let mut stack = vec![owner.to_string()];
    let mut seen: HashSet<String> = HashSet::new();

    while let Some(binary) = stack.pop() {
        if !seen.insert(binary.clone()) {
            continue;
        }
        // Bytecode nests with `$` (`java/util/Map$Entry`); a project-source type is keyed
        // off its dotted FQN, so with `/` (`com/acme/Base/Inner`). Which one exists depends
        // on whether the owner came out of a jar or out of the project, so probe both — the
        // resolver memoizes misses, so the second probe is not a second lookup for long.
        for candidate in [format!("{binary}${simple}"), format!("{binary}/{simple}")] {
            if resolver.members_of(&candidate).is_some() {
                return Some(candidate);
            }
        }
        let Some(members) = resolver.members_of(&binary) else {
            // An un-indexed ancestor: we cannot see through it, and the caller's
            // conservative reading of `None` is what keeps that from becoming a false
            // "cannot resolve".
            continue;
        };
        if let Some(superclass) = &members.superclass {
            stack.push(superclass.clone());
        }
        // Interfaces too: a member type of an implemented interface is inherited the
        // same way, and constant/type holders written as interfaces are a legacy staple.
        stack.extend(members.interfaces.iter().cloned());
    }
    None
}

/// The binary name a bare `simple` type would have IF it lives in the file's own package
/// (`com.acme` + `C` → `com/acme/C`). `None` for a file with no / an empty package declaration (a
/// default-package type is keyed by its bare name, already covered by the resolver's simple lookup).
pub fn same_package_binary(simple: &str, symbols: &FileSymbols) -> Option<String> {
    let pkg = symbols.package.as_deref()?;
    if pkg.is_empty() {
        return None;
    }
    Some(format!("{}/{}", pkg.replace('.', "/"), simple))
}
