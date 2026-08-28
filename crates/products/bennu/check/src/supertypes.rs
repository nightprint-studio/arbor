//! One reading of a type declaration's HEADER: its `extends` and `implements` clauses, and the
//! scope their names are resolved in.
//!
//! There were five copies of the traversal — `inheritance`, `iface_dup`, `method_sig`,
//! `constructors` and `super_method` each carried its own `superclass_text` / `implements_types` —
//! and they disagreed on which node kinds count as a written type, so whether a supertype was seen
//! at all depended on which check was asking.
//!
//! They also all resolved the names in the wrong SCOPE, in the same way. A member type's scope is
//! the **body** of its class (JLS §6.3), and a header is not in the body: `class HashCodeBuilder
//! implements Builder<Integer>` in a class that also declares a nested `Builder` names the
//! same-package *interface*, which is why javac compiles commons-lang. Reading the header inside
//! the body bound it to the nested class, and six classes were reported as implementing something
//! that is not an interface — plus their overrides, plus their covariant returns.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use tree_sitter::Node;

use crate::nodes::{is_class_type_node, text};
use crate::type_scope::TypeScope;

/// A supertype as it is WRITTEN: the source text (`Builder<Integer>`) and the node it sits at.
pub(crate) struct Written<'t> {
    pub(crate) text: String,
    pub(crate) node: Node<'t>,
}

/// The `extends` type of a CLASS declaration — enums and records cannot declare one, and an
/// interface's `extends` list is [`interfaces`], not this.
pub(crate) fn superclass<'t>(decl: Node<'t>, bytes: &[u8]) -> Option<Written<'t>> {
    let wrapper = decl.child_by_field_name("superclass")?;
    let mut c = wrapper.walk();
    let found = wrapper
        .named_children(&mut c)
        .find(|ch| is_class_type_node(ch.kind()))?;
    written(found, bytes)
}

/// The interfaces a declaration lists: `implements I, J` on a class / enum / record, or
/// `extends I, J` on an interface. One function because they are the same relation written twice.
pub(crate) fn interfaces<'t>(decl: Node<'t>, bytes: &[u8]) -> Vec<Written<'t>> {
    let wrapper = decl.child_by_field_name("interfaces").or_else(|| {
        let mut c = decl.walk();
        let found = decl
            .named_children(&mut c)
            .find(|ch| ch.kind() == "extends_interfaces");
        found
    });
    let Some(wrapper) = wrapper else { return Vec::new() };
    let mut out = Vec::new();
    collect(wrapper, bytes, &mut out);
    out
}

/// Every written supertype of `decl`: the superclass first, then the interfaces.
pub(crate) fn all<'t>(decl: Node<'t>, bytes: &[u8]) -> Vec<Written<'t>> {
    superclass(decl, bytes)
        .into_iter()
        .chain(interfaces(decl, bytes))
        .collect()
}

/// The scope a type declaration's header is read in: the type ENCLOSING `decl`, or the compilation
/// unit when `decl` is top-level. Never `decl` itself — that is the rule this module exists for.
pub(crate) fn header_scope(decl: Node, bytes: &[u8], symbols: &FileSymbols) -> TypeScope {
    crate::resolve::enclosing_scope(decl, bytes, symbols)
}

/// The binary name a supertype written in `decl`'s header denotes, or `None` when nothing binds it
/// (every caller treats that conservatively — skip).
pub(crate) fn binary(
    written: &str,
    decl: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<String> {
    crate::resolve::type_binary_in(written, header_scope(decl, bytes, symbols), symbols, resolver)
}

/// The direct supertypes of `decl` that RESOLVE, in written order (superclass first).
///
/// Unresolvable names are DROPPED. That is what a check walking a hierarchy for something it might
/// report wants: it can only judge what it can read, and a supertype it cannot read simply
/// contributes nothing.
pub(crate) fn binaries(
    decl: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<String> {
    all(decl, bytes)
        .iter()
        .filter_map(|sup| binary(&sup.text, decl, bytes, symbols, resolver))
        .collect()
}

/// The same, but `None` as soon as ONE written supertype fails to resolve.
///
/// For a check whose verdict needs the WHOLE set — "this class implements no `run()`" is only true
/// if every supertype was read, since the unread one may be exactly where it is declared.
pub(crate) fn binaries_complete(
    decl: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    all(decl, bytes)
        .iter()
        .map(|sup| binary(&sup.text, decl, bytes, symbols, resolver))
        .collect()
}

/// Descend to every written type under a `type_list` wrapper, in source order.
fn collect<'t>(node: Node<'t>, bytes: &[u8], out: &mut Vec<Written<'t>>) {
    if is_class_type_node(node.kind()) {
        out.extend(written(node, bytes));
        return;
    }
    let mut c = node.walk();
    for ch in node.named_children(&mut c) {
        collect(ch, bytes, out);
    }
}

fn written<'t>(node: Node<'t>, bytes: &[u8]) -> Option<Written<'t>> {
    text(node, bytes).map(|text| Written { text, node })
}
