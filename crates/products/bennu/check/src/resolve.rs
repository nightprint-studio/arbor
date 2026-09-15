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
    if text.trim().is_empty() {
        return None;
    }
    // One reading of a written type name for the whole workspace — see `bennu_java::typename`. This
    // used to be its own copy, and it carried the two bugs every copy carried: `Outer.Nested` read
    // as an already-qualified name, and no notion of a member type inherited from a supertype.
    // `None` means "nothing bound this name" — the contract every caller reads, and now the one
    // the shared resolver states in its return type rather than leaving to be inferred from the
    // shape of a string.
    type_binary_in(text, crate::type_scope::TypeScope::Unknown, symbols, resolver)
}

/// [`type_binary`], told exactly WHERE the name is written — the one entry point that takes a
/// [`TypeScope`](crate::type_scope::TypeScope). The two wrappers above pick a scope for their
/// callers; anything with a real position should come through here.
pub fn type_binary_in(
    text: &str,
    scope: crate::type_scope::TypeScope,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    // The full written text, NOT its prefix before the first `<`. Truncating there throws away a
    // nested type named through a parameterised qualifier — guava writes
    // `extends AbstractMultiset<E>.EntrySet`, which came out as `AbstractMultiset`, an abstract
    // class whose six abstract methods were then all reported as unimplemented. The argument lists
    // are erased segment by segment inside `resolve_written_type`.
    bennu_java::prelude::resolve_written_type(
        text,
        &crate::type_scope::FileScope { symbols, resolver, scope },
    )
    .resolved()
    .filter(|b| bennu_java::prelude::is_resolved_binary(b, resolver))
}

/// [`type_binary`], told which type the name was written INSIDE.
///
/// The owner decides which supertype chain an inherited member type is taken from — see
/// [`crate::type_scope::FileScope::owner`]. Prefer this wherever the caller holds the node.
pub fn type_binary_at(
    text: &str,
    node: tree_sitter::Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    type_binary_in(text, enclosing_scope(node, bytes, symbols), symbols, resolver)
}

/// The binary type a `recv.method(…)` call is made ON: the receiver VALUE's inferred type, else —
/// for a static call `Util.convert(…)` — the TYPE the receiver names. `None` for a bare call.
///
/// Inference types values only, so the receiver of every static call came back untyped and the
/// call went unjudged by the checks reading it (argument count and argument types alike). The type
/// reading is taken only where no value can be meant — a variable obscures a type of the same name
/// (JLS §6.4.2) — so the receiver's head must be declared by no enclosing local or parameter and by
/// no field of this file, its last segment must be capitalised, and the type must be readable.
pub(crate) fn call_receiver_binary(
    call: tree_sitter::Node,
    root: &tree_sitter::Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &bennu_java::prelude::InferCache,
) -> Option<String> {
    let obj = call.child_by_field_name("object")?;
    let inferred =
        bennu_java::prelude::infer_node_type_cached(root, source, symbols, &obj, resolver, cache);
    match inferred {
        Some(ty) => (!ty.binary_name.is_empty()).then_some(ty.binary_name),
        None => static_receiver_binary(obj, source.as_bytes(), symbols, resolver),
    }
}

/// The type a receiver like `Util` / `java.util.Collections` / `Outer.Inner` names, when it cannot
/// be a value — see [`call_receiver_binary`].
fn static_receiver_binary(
    obj: tree_sitter::Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    let head = leftmost_identifier(obj)?;
    let text = obj.utf8_text(bytes).ok()?;
    let last = text.rsplit('.').next()?.trim();
    if !last.chars().next()?.is_uppercase() {
        return None;
    }
    let head_name = head.utf8_text(bytes).ok()?;
    let is_file_field = symbols.types.iter().any(|t| t.fields.iter().any(|f| f.name == head_name));
    if is_file_field || crate::scopes::resolves_as_local(head, outermost_type(obj)?, bytes) {
        return None;
    }
    let binary = type_binary_at(text, obj, bytes, symbols, resolver)?;
    resolver.members_of(&binary).map(|_| binary)
}

/// The first identifier of a dotted receiver: `a` in `a.b.C`.
fn leftmost_identifier(mut node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    loop {
        match node.kind() {
            "identifier" => return Some(node),
            "field_access" => node = node.child_by_field_name("object")?,
            "scoped_identifier" => node = node.child_by_field_name("scope")?,
            _ => return None,
        }
    }
}

/// The top-level declaration `node` sits in — the node whose body ends a locals search.
fn outermost_type(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.parent().is_none() {
            return Some(cur);
        }
        cur = parent;
    }
    None
}

/// The scope a name written AT `node` is read in: the body of the type that encloses it, or the
/// compilation unit when nothing does.
///
/// `enclosing_type_fqn` starts at the node's PARENT, so passing a type declaration itself yields
/// the scope its HEADER is read in — which is the whole point of the distinction (see
/// [`TypeScope`](crate::type_scope::TypeScope)).
pub fn enclosing_scope(
    node: tree_sitter::Node,
    bytes: &[u8],
    symbols: &FileSymbols,
) -> crate::type_scope::TypeScope {
    match bennu_java::prelude::enclosing_type_fqn(&node, bytes, symbols) {
        Some(fqn) => crate::type_scope::TypeScope::Inside(fqn.replace('.', "/")),
        None => crate::type_scope::TypeScope::CompilationUnit,
    }
}

/// A member type named `simple` **inherited** by `owner` (a binary name), searching its supertype
/// chain — or declared on `owner` itself.
///
/// A nested type declared in a superclass or superinterface is in scope in the subclass by its
/// simple name, with no import (JLS §8.1.5): `class Sub extends Base` writes `Inner` and means
/// `Base.Inner`. Nobody imports it, because there is nothing to import.
///
/// Delegates to [`bennu_java::prelude::inherited_member_type_of`]: this was a second copy of that
/// walk, and the two knew different things — this one was never consulted when resolving a written
/// type name, so the rule it exists for did not apply there.
pub fn inherited_member_type(
    owner: &str,
    simple: &str,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    bennu_java::prelude::inherited_member_type_of(resolver, owner, simple)
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
