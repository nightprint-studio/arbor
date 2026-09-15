//! Assigning a `final` field through a qualifier — `Math.PI = 3`, `values.length = 0`,
//! `other.id = 1` — javac's `cant.assign.val.to.var`.
//!
//! [`crate::flow::finals`] judges the `final` fields a type declares, inside that type's own members,
//! where the JLS lets a blank final be assigned exactly once. Reached through a qualifier from
//! anywhere else, a `final` field is never assignable — and whether a field IS final is written on
//! its declaration, which for a JDK or library type only the index has. Hence a check of its own.
//!
//! Silent unless the receiver's type is known and the field is found `final` on it. A field declared
//! by a type the site is written inside is left to `finals`, whose blank-final rules those are.

use bennu_java::prelude::{
    infer_node_type_cached, walk, FileSymbols, InferCache, MemberKind, TypeRef, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// Every assignment or increment of a `final` field reached through a qualifier.
pub fn qualified_final_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        let target = match n.kind() {
            "assignment_expression" => n.child_by_field_name("left"),
            "update_expression" => n.named_child(0),
            _ => None,
        };
        let Some(target) = target.filter(|t| t.kind() == "field_access") else { continue };
        let site = Site { root: &root, source, bytes, symbols, resolver, cache };
        if let Some(message) = site.final_field_message(target) {
            out.push(CheckId::FinalAssignment.at(target, message));
        }
    }
    out
}

/// What reading a receiver's type needs.
struct Site<'a, 't> {
    root: &'a Node<'t>,
    source: &'a str,
    bytes: &'a [u8],
    symbols: &'a FileSymbols,
    resolver: &'a dyn TypeResolver,
    cache: &'a InferCache,
}

impl Site<'_, '_> {
    /// Why assigning `target` is illegal, when it provably is.
    fn final_field_message(&self, target: Node) -> Option<String> {
        let object = target.child_by_field_name("object")?;
        let field = target.child_by_field_name("field").filter(|f| f.kind() == "identifier")?;
        if matches!(object.kind(), "this" | "super") || has_super(target) {
            return None;
        }
        let name = field.utf8_text(self.bytes).ok()?;
        let ty = match infer_node_type_cached(self.root, self.source, self.symbols, &object, self.resolver, self.cache) {
            Some(ty) if !ty.binary_name.is_empty() => ty,
            // Not a value: a type, and this a static field — `Math.PI`.
            _ => TypeRef::simple(crate::support::resolve::static_receiver_binary(
                object,
                self.bytes,
                self.symbols,
                self.resolver,
            )?),
        };
        if ty.is_array() {
            return (name == "length")
                .then(|| "Cannot assign a value to final variable `length` — an array's length is fixed".to_string());
        }
        // The nearest declaration wins: a field of a subclass hides one of the same name above it.
        let found = walk(self.resolver, &TypeRef::simple(ty.binary_name.clone()), |a| {
            a.members
                .fields
                .iter()
                .find(|f| f.name == name && f.kind == MemberKind::Field)
                .map(|f| (a.ty.binary_name.clone(), f.is_final))
        })
        .found?;
        let (owner, true) = found else { return None };
        if written_inside(target, &owner, self.bytes) {
            return None;
        }
        Some(format!("Cannot assign a value to final variable `{name}`"))
    }
}

/// Whether a `field_access` goes through `super` — `Outer.super.f`.
fn has_super(n: Node) -> bool {
    let mut c = n.walk();
    let found = n.children(&mut c).any(|ch| ch.kind() == "super");
    found
}

/// Whether `node` sits inside a declaration of the type `owner`. By simple name, which can only err
/// towards silence.
fn written_inside(node: Node, owner: &str, bytes: &[u8]) -> bool {
    let simple = owner.rsplit(['/', '$']).next().unwrap_or(owner);
    let mut cur = node.parent();
    while let Some(n) = cur {
        let is_type = matches!(
            n.kind(),
            "class_declaration" | "enum_declaration" | "record_declaration" | "interface_declaration"
        );
        if is_type && n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) == Some(simple) {
            return true;
        }
        cur = n.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver(HashMap<String, ClassMembers>);

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            (name == "Limits").then(|| "acme/Limits".to_string())
        }
    }

    fn resolver() -> MapResolver {
        let limits = ClassMembers {
            type_params: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: vec![
                Member::field("MAX", TypeRef::simple("int")).stat().final_(),
                Member::field("current", TypeRef::simple("int")).stat(),
            ],
            flags: Default::default(),
        };
        MapResolver([("acme/Limits".to_string(), limits)].into_iter().collect())
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("package p;\nclass C {{ void m(int[] values) {{ {body} }} }}");
        let tree = bennu_java::prelude::parse_java(&src).unwrap();
        let symbols = bennu_java::prelude::extract_symbols(&src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        qualified_final_errors_in(tree.root_node(), &nodes, &src, &symbols, &resolver(), &InferCache::new())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn a_final_static_field_of_another_type_is_not_assignable() {
        let d = diags("Limits.MAX = 3;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`MAX`"), "{d:?}");
        assert_eq!(diags("Limits.MAX++;").len(), 1);
    }

    #[test]
    fn a_field_that_is_not_final_is_fine() {
        assert!(diags("Limits.current = 3;").is_empty());
    }

    #[test]
    fn an_array_length_is_not_assignable() {
        let d = diags("values.length = 3;");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
