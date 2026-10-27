//! A method reference to a method that throws what its functional interface does not allow
//! (resolver-backed, JLS §15.13.2): `Runnable task = this::readFile;` with `void readFile() throws
//! IOException` is `compiler.err.incompatible.thrown.types.in.mref` — `Runnable.run` declares nothing,
//! and a method reference cannot catch.
//!
//! Judged for the plain shape, a method reference initialising a local or a field of a functional
//! interface type, and only on evidence that is certain:
//!   * the interface resolves and has one abstract method, none of whose `throws` entries is a type
//!     variable (`Callable<V>` throws `Exception`, `ThrowingSupplier<T, E>` throws `E`);
//!   * the referenced method is found on a receiver the reference names (`this`, a variable, or a type),
//!     whose hierarchy is fully known, and **every** overload that could bind throws a checked exception
//!     that nothing the interface declares covers;
//!   * the exception's own hierarchy is fully known, so "checked" and "not covered" are facts.

use bennu_java::prelude::{
    infer_node_type_cached, single_abstract_method, FileSymbols, InferCache, Member, MemberKind, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::nodes::is_type_var;
use crate::support::resolve::{type_binary, written_type_binary};
use crate::support::walk::{hierarchy_fully_known, reaches};
use crate::throwing::checked_throw::is_checked;

/// All method references that throw more than their target allows.
pub fn mref_throws_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes.iter().filter(|n| n.kind() == "method_reference" && !n.has_error()) {
        if let Some(message) = check_reference(n, &root, source, symbols, resolver, cache) {
            out.push(CheckId::UnhandledCheckedException.at(n, message));
        }
    }
    out
}

fn check_reference(
    reference: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<String> {
    let bytes = source.as_bytes();
    let target = declared_target(reference, bytes, symbols, resolver)?;
    let sam = single_abstract_method(resolver, &TypeRef::simple(target))?;
    if sam.throws.iter().any(|t| is_type_var(t)) {
        return None;
    }
    let (receiver, name) = receiver_and_name(reference, root, source, symbols, resolver, cache)?;
    let res = cache.resolve_methods(resolver, &receiver, name);
    if !res.complete {
        return None;
    }
    // A reference binds a method taking all of the SAM's arguments (static, or on a bound receiver) or
    // all but the first (an unbound receiver): both counts are admitted so none is missed.
    let arity = sam.params.len();
    let admitted: Vec<&Member> = res
        .candidates
        .iter()
        .filter(|m| m.kind == MemberKind::Method && (m.params.len() == arity || m.params.len() + 1 == arity))
        .collect();
    if admitted.is_empty() {
        return None;
    }
    let mut uncovered: Option<String> = None;
    for m in admitted {
        let exception = uncovered_checked(m, &sam, resolver)?;
        uncovered.get_or_insert(exception);
    }
    uncovered.map(|exception| {
        format!(
            "`{name}` throws `{}`, which the functional interface does not declare",
            exception.rsplit(['/', '$']).next().unwrap_or(&exception)
        )
    })
}

/// The first checked exception `method` throws that nothing in `sam`'s `throws` covers — `None` when
/// there is none, or when anything about it is not certain (a type variable, a gap in a hierarchy).
fn uncovered_checked(method: &Member, sam: &Member, resolver: &dyn TypeResolver) -> Option<String> {
    method.throws.iter().find_map(|thrown| {
        let certain = !is_type_var(thrown) && hierarchy_fully_known(resolver, thrown);
        let covered = sam.throws.iter().any(|allowed| reaches(resolver, thrown, allowed));
        (certain && is_checked(resolver, thrown) && !covered).then(|| thrown.clone())
    })
}

/// The binary name of the functional interface a method reference initialises: the written type of the
/// local or field declaration around it. Nothing else — an argument or a return is not read.
fn declared_target(reference: Node, bytes: &[u8], symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<String> {
    let declarator = reference.parent().filter(|p| {
        p.kind() == "variable_declarator" && p.child_by_field_name("value").is_some_and(|v| v.id() == reference.id())
    })?;
    let declaration = declarator.parent().filter(|d| matches!(d.kind(), "local_variable_declaration" | "field_declaration"))?;
    let ty = declaration.child_by_field_name("type")?;
    if !matches!(ty.kind(), "type_identifier" | "generic_type" | "scoped_type_identifier") {
        return None;
    }
    written_type_binary(ty, bytes, symbols, resolver)
}

/// The type the reference names or evaluates to on its left, and the method name on its right.
fn receiver_and_name<'s>(
    reference: Node,
    root: &Node,
    source: &'s str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Option<(String, &'s str)> {
    let bytes = source.as_bytes();
    let mut c = reference.walk();
    let children: Vec<Node> = reference.named_children(&mut c).collect();
    let (left, right) = (children.first()?, children.last()?);
    if left.id() == right.id() || right.kind() != "identifier" {
        return None;
    }
    let name = right.utf8_text(bytes).ok()?;
    let receiver = match left.kind() {
        "this" | "identifier" | "field_access" | "method_invocation" => {
            // A variable or a value is typed by inference; a bare identifier that names a TYPE is not
            // a variable, and resolves as one.
            infer_node_type_cached(root, source, symbols, left, resolver, cache)
                .filter(|t| !t.binary_name.is_empty() && t.dims == 0)
                .map(|t| t.binary_name)
                .or_else(|| (left.kind() == "identifier").then(|| type_binary(left.utf8_text(bytes).ok()?, symbols, resolver)).flatten())?
        }
        "type_identifier" | "scoped_type_identifier" => type_binary(left.utf8_text(bytes).ok()?, symbols, resolver)?,
        _ => return None,
    };
    Some((receiver, name))
}
