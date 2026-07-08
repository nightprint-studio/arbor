//! Shared helper: resolve a *written* type name (as it appears in source — `Foo`, `Foo<Bar>`,
//! `com.acme.Foo`) to a JVM binary name, using the file's imports + same-file declarations + the
//! resolver. Used by the constructor-arity and inheritance checks, so the resolution rules live once.

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
