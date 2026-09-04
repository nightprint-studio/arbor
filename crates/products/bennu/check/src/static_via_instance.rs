//! A `static` method called through an instance — `connection.getInstance()`, `helper.parse(s)` —
//! javac's `compiler.warn.static.not.qualified.by.type`.
//!
//! It compiles, which is exactly why it is worth reporting: the receiver is evaluated and then
//! thrown away, so the code reads as though the answer depends on *that* object when it cannot. The
//! two ways it goes wrong in practice are worth the noise: a receiver that is `null` still works,
//! which hides a bug until someone rewrites the line; and a subclass's name on the receiver suggests
//! an override that `static` never does.
//!
//! **Only a receiver that is certainly a VALUE is judged.** `Files.readAllLines(p)` is a type
//! qualifier and correct; telling the two apart is the whole difficulty, because both are an
//! identifier followed by a dot. So the receiver must be one of:
//!   * `new Foo().m()` / `getFoo().m()` / `this.m()` — a value by construction;
//!   * a name that is declared as a local, parameter, `for` variable, `catch` parameter, resource or
//!     pattern variable in an enclosing scope — the one identifier shape that cannot be a type name.
//!
//! A field receiver is deliberately left out: `Config.INSTANCE.m()` and `config.m()` are both a
//! `field_access`, and separating them needs to know which names the enclosing type declares.
//!
//! And it is flagged only when **every** method of that name in a **fully known** hierarchy is
//! static — an overload set with an instance member could be binding to that one instead.

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::simple_name;

/// Every static-through-an-instance call in the pre-collected node slice.
pub fn static_via_instance_warnings_in(
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
        if n.kind() == "method_invocation" {
            check_call(n, &root, source, bytes, symbols, resolver, cache, &mut out);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn check_call(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(obj) = n.child_by_field_name("object") else { return };
    let Some(name_node) = n.child_by_field_name("name") else { return };
    if name_node.has_error() || !is_certainly_a_value(obj, bytes) {
        return;
    }
    let Ok(method) = name_node.utf8_text(bytes) else { return };

    let Some(ty) = bennu_java::prelude::infer_node_type_cached(root, source, symbols, &obj, resolver, cache)
    else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }
    // The same memoized hierarchy walk the unknown-member and arity checks share.
    let res = cache.resolve_methods(resolver, &ty.binary_name, method);
    // An incomplete hierarchy could hold an instance method of this name in a base we cannot see,
    // and that one would be what the call binds to.
    if !res.complete || res.candidates.is_empty() {
        return;
    }
    if res.candidates.iter().all(|m| m.is_static) {
        out.push(CheckId::StaticViaInstance.at(
            name_node,
            format!(
                "`{method}` is static — call it as `{}.{method}`; the receiver is evaluated and \
                 discarded",
                simple_name(&ty.binary_name)
            ),
        ));
    }
}

/// Whether this receiver is a value rather than a type name.
///
/// The conservative half of the check. Everything not listed here — a bare uppercase identifier, a
/// `field_access`, a qualified `a.b.C` — is skipped, because a type qualifier written that way is
/// correct code and flagging it would be worse than missing the instance case beside it.
fn is_certainly_a_value(obj: Node, bytes: &[u8]) -> bool {
    match obj.kind() {
        // A value by construction: nothing here can name a type.
        "object_creation_expression"
        | "method_invocation"
        | "array_access"
        | "this"
        | "parenthesized_expression"
        | "cast_expression"
        | "string_literal" => true,
        // The one identifier shape that cannot be a type: a name some enclosing scope declares as a
        // variable. Uses the same scope walk the undefined-variable check does, rather than a second
        // copy of it — over-collection there only ever suppresses, which is the safe direction here
        // too (a suppressed finding, never an invented one).
        "identifier" => enclosing_type(obj)
            .is_some_and(|top| crate::scopes::resolves_as_local(obj, top, bytes)),
        _ => false,
    }
}

/// The type declaration enclosing `n` — the boundary the scope walk stops at.
fn enclosing_type(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if matches!(
            p.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, MemberKind, TypeRef, Visibility};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn m(name: &str, is_static: bool) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple("void"),
            params: Vec::new(),
            is_static,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature: String::new(),
            throws: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn ty(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some(TypeRef::simple("java/lang/Object")),
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// `Helper` has a static `parse` and an instance `run`; `Mixed` has BOTH a static and an
    /// instance `pick`, so the call could be binding to either.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), ty(Vec::new()));
        members.insert(
            "com/acme/Helper".into(),
            ty(vec![m("parse", true), m("run", false)]),
        );
        members.insert(
            "com/acme/Mixed".into(),
            ty(vec![m("pick", true), m("pick", false)]),
        );
        let simple = [
            ("Helper", "com/acme/Helper"),
            ("Mixed", "com/acme/Mixed"),
            ("Object", "java/lang/Object"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = bennu_java::prelude::extract_symbols_from_root(&root, src);
        let cache = InferCache::new();
        static_via_instance_warnings_in(root, &nodes, src, &symbols, &resolver(), &cache)
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn a_static_method_called_on_a_local_is_flagged() {
        let src = "class A { void go() { Helper h = null; h.parse(); } }";
        assert_eq!(codes(src), ["static-via-instance"]);
    }

    #[test]
    fn a_static_method_called_on_a_parameter_is_flagged() {
        let src = "class A { void go(Helper h) { h.parse(); } }";
        assert_eq!(codes(src), ["static-via-instance"]);
    }

    #[test]
    fn a_static_method_called_on_a_fresh_instance_is_flagged() {
        let src = "class A { void go() { new Helper().parse(); } }";
        assert_eq!(codes(src), ["static-via-instance"]);
    }

    /// The case the check exists to NOT do: a type qualifier is the correct way to write this.
    #[test]
    fn calling_it_through_the_type_is_correct_and_silent() {
        assert!(codes("class A { void go() { Helper.parse(); } }").is_empty());
    }

    #[test]
    fn an_instance_method_on_an_instance_is_silent() {
        assert!(codes("class A { void go(Helper h) { h.run(); } }").is_empty());
    }

    /// An overload set holding an instance method could be binding to that one.
    #[test]
    fn a_name_with_both_a_static_and_an_instance_overload_is_left_alone() {
        assert!(codes("class A { void go(Mixed x) { x.pick(); } }").is_empty());
    }

    /// An unknown receiver type says nothing about the method.
    #[test]
    fn an_unresolvable_receiver_is_silent() {
        assert!(codes("class A { void go(Whatever w) { w.parse(); } }").is_empty());
    }

    /// A bare uppercase identifier that is not a declared variable is treated as a type name, which
    /// is what it almost always is.
    #[test]
    fn an_undeclared_identifier_receiver_is_treated_as_a_type() {
        assert!(codes("class A { void go() { Helper.parse(); } }").is_empty());
    }
}
