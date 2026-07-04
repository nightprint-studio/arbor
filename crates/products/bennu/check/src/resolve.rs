//! Shared helper: resolve a *written* type name (as it appears in source — `Foo`, `Foo<Bar>`,
//! `com.acme.Foo`) to a JVM binary name, using the file's imports + same-file declarations + the
//! resolver. Used by the constructor-arity and inheritance checks, so the resolution rules live once.

use bennu_java::prelude::{FileSymbols, TypeResolver};

/// Resolve `text` (a type as written) to a binary name (`com/acme/Foo`). Strips generic arguments.
/// `None` when unresolvable — callers treat that conservatively (skip).
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
    resolver.resolve_simple_name(simple, &symbols.imports)
}
