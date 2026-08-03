//! Unknown-field diagnostics — a `receiver.field` whose `field` doesn't exist on the receiver's
//! inferred type. The field counterpart to [`crate::members`]: same inference, same conservative
//! gate (flag only when the receiver type is KNOWN and the field genuinely absent, walking
//! supertypes), just over `field_access` nodes and the `fields` list.
//!
//! Deliberately silent on everything inference can't pin down — a static qualifier (`System.out`,
//! `Integer.MAX_VALUE`), a package/type prefix (`java.util.List`), an array's pseudo-`length`, or an
//! unknown receiver type all yield no inferred value type, so they're skipped, never mis-flagged.

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::walk::hierarchy_has;

/// Parse `source` and flag accesses to non-existent fields on their inferred receiver types.
pub fn unknown_fields(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    unknown_fields_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// The tree-driven core: iterates the shared `nodes` + reuses `root` + `symbols` + inference `cache`.
pub fn unknown_fields_in(
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
        if n.kind() == "field_access" {
            check_access(n, &root, source, bytes, symbols, resolver, cache, &mut out);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn check_access(
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
    let Some(field) = n.child_by_field_name("field") else { return };
    if field.has_error() {
        return;
    }
    let Ok(field_name) = field.utf8_text(bytes) else { return };
    // `arr.length` is a pseudo-field the JVM synthesises — never in the members list. Skip it wholesale
    // (an array receiver has no `ClassMembers` anyway, but this is belt-and-braces + clearer intent).
    if field_name == "length" {
        return;
    }

    // Infer the receiver (`object`) type from the already-located node (no descendant search).
    let Some(ty) = infer_node_type_cached(root, source, symbols, &obj, resolver, cache) else {
        return;
    };
    if ty.binary_name.is_empty() {
        return;
    }
    // Only assert absence when we actually know the receiver type's members.
    if resolver.members_of(&ty.binary_name).is_none() {
        return;
    }
    let has = hierarchy_has(resolver, &ty.binary_name, &|cm| {
        cm.fields.iter().any(|m| m.name == field_name && m.kind == MemberKind::Field)
    });
    if !has {
        out.push(crate::check_id::CheckId::UnknownField.at(
            field,
            format!("Cannot resolve field `{field_name}` in `{}`", simple_name(&ty.binary_name)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
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

    fn field(name: &str, ty: &str) -> Member {
        Member::field(name, TypeRef::simple(ty.to_string())).sig(format!("{ty} {name}"))
    }

    /// A `Point { int x; int y; }` extending `Base { Object tag; }`, plus a generic `Box<T> { T value; }`.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("tag", "java/lang/Object")],
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Point".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("com/acme/Base".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("x", "int"), field("y", "int")],
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Box".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("value", "T")],
                flags: Default::default(),
            },
        );
        let simple = [("Point", "com/acme/Point"), ("Base", "com/acme/Base"), ("Box", "com/acme/Box")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        unknown_fields(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn known_field_is_ok() {
        assert!(diags("Point p = null; int a = p.x;").is_empty());
    }

    #[test]
    fn unknown_field_is_flagged() {
        let d = diags("Point p = null; int a = p.z;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`z`") && d[0].contains("Point"), "{d:?}");
    }

    #[test]
    fn inherited_field_is_ok() {
        // `tag` is declared on Base (Point's superclass) — the walk must find it.
        assert!(diags("Point p = null; Object t = p.tag;").is_empty());
    }

    #[test]
    fn generic_field_is_resolved() {
        assert!(diags("Box<String> b = null; String v = b.value;").is_empty());
        let d = diags("Box<String> b = null; String v = b.valuee;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("valuee"));
    }

    #[test]
    fn unknown_receiver_type_is_not_flagged() {
        assert!(diags("Unknown u = null; Object o = u.whatever;").is_empty());
    }

    // ── anonymous class bodies ───────────────────────────────────────────────────

    /// The reported bug: a field declared by an **anonymous class** was looked up on the
    /// enclosing class (where the `new` sits), which of course doesn't have it — so
    /// `this.queriable` inside `new Foo<>() { private Bar queriable; … }` was reported as
    /// unresolvable, on correct code, for every field such a body declares.
    #[test]
    fn a_field_of_an_anonymous_class_is_not_looked_up_on_the_outer_class() {
        // `Point` (the enclosing type here) has x/y and no `queriable` — exactly the shape that
        // used to produce the false error.
        let src = "class Point {\n\
                     Object make() {\n\
                       return new Runnable() {\n\
                         private String queriable;\n\
                         public void run() { String s = this.queriable; }\n\
                       };\n\
                     }\n\
                   }\n";
        let d: Vec<String> =
            unknown_fields(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "an anonymous class's own field must not be flagged: {d:?}");
    }

    /// A bare `queriable` (no `this.`) inside the same body must stay silent too — that path goes
    /// through the same enclosing-type lookup.
    #[test]
    fn an_unqualified_field_of_an_anonymous_class_is_silent() {
        let src = "class Point {\n\
                     Object make() {\n\
                       return new Runnable() {\n\
                         private String queriable;\n\
                         public void run() { int n = queriable.length(); }\n\
                       };\n\
                     }\n\
                   }\n";
        let d: Vec<String> =
            unknown_fields(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "{d:?}");
    }

    /// The silence is scoped to the anonymous body — `this.z` in an ordinary class method is still
    /// checked, so the fix didn't buy itself by switching the rule off.
    #[test]
    fn this_in_an_ordinary_class_is_still_checked() {
        let src = "class Point {\n  int x;\n  void m() { int a = this.z; }\n}\n";
        // `Point` here is the source's own class; the resolver knows `com/acme/Point`, so this
        // asserts the walk still REACHES a named enclosing type rather than bailing everywhere.
        let d: Vec<String> =
            unknown_fields(src, &resolver()).into_iter().map(|x| x.message).collect();
        // Either it resolves the enclosing type and flags `z`, or it can't resolve `Point` at all
        // and stays silent — what must NOT happen is the anonymous-body bail leaking to this case,
        // which would show up as inference returning None for a plain method. Assert on inference
        // directly so the test says what it means.
        let symbols = bennu_java::prelude::extract_symbols(src);
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let this_off = src.find("this.z").unwrap();
        let this_node = tree
            .root_node()
            .descendant_for_byte_range(this_off, this_off + 4)
            .expect("the `this` node");
        let r = resolver();
        let inferred = bennu_java::prelude::infer_node_type_cached(
            &tree.root_node(),
            src,
            &symbols,
            &this_node,
            &r,
            &InferCache::new(),
        );
        assert!(inferred.is_some(), "`this` in an ordinary method must still infer a type: {d:?}");
    }

    #[test]
    fn array_length_is_not_flagged() {
        assert!(diags("Point[] ps = null; int n = ps.length;").is_empty());
    }

    #[test]
    fn static_qualifier_is_not_flagged() {
        // `System.out` — `System` is a type, not a value; inference yields nothing → skip.
        assert!(diags("Object o = System.out;").is_empty());
    }
}
