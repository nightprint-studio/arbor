//! What an `@interface` may declare an element to be — javac's
//! `compiler.err.invalid.annotation.member.type`.
//!
//! The legal set is short and closed (JLS §9.6.1): a primitive, `String`, `Class` (parameterised or
//! not), an enum type, an annotation type, or a **one-dimensional** array of one of those. Anything
//! else — `Object`, a `List<String>`, a class of your own, `void`, an array of arrays — is rejected
//! at the declaration, before any use site exists.
//!
//! Worth its own check because the error it prevents arrives in the wrong place. An element typed
//! `MyObj[]` reads perfectly, and every use of it then looks like a bad *value* (`OBJ` is `final`,
//! so why is it refused?) when the declaration is what is wrong.
//!
//! Conservative in the one way it could be noisy: a type that does not resolve says nothing about
//! which family it belongs to, so it is skipped rather than guessed.

use bennu_java::prelude::{split_array_dims, FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::nodes::is_primitive;
use crate::resolve::type_binary_at;

/// Every `@interface` element declared with a type an annotation element cannot have.
pub fn annotation_decl_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "annotation_type_element_declaration" {
            check_element(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_element(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(type_node) = n.child_by_field_name("type") else { return };
    let Ok(written) = type_node.utf8_text(bytes) else { return };
    // `String a()[]` puts the dimension after the NAME — the same array, spelled the old way.
    let trailing = n
        .child_by_field_name("dimensions")
        .and_then(|d| d.utf8_text(bytes).ok())
        .map(|t| t.matches('[').count())
        .unwrap_or(0);
    let (base, dims) = split_array_dims(written);
    let dims = dims + trailing;

    let bad = |what: &str| {
        Some(format!(
            "`{}` is not a valid annotation element type — {what}",
            written.trim()
        ))
    };
    let problem = if dims > 1 {
        bad("an element array has exactly one dimension")
    } else if base.trim() == "void" {
        bad("an element has to return something")
    } else if is_primitive(base) {
        None
    } else {
        match type_binary_at(base, n, bytes, symbols, resolver) {
            // Unresolved: which family it belongs to is exactly what we don't know.
            None => None,
            Some(binary) if binary == "java/lang/String" || binary == "java/lang/Class" => None,
            Some(binary) => match resolver.members_of(&binary) {
                None => None,
                Some(cm) if cm.flags.is_enum || cm.flags.is_annotation => None,
                Some(_) => bad(
                    "only a primitive, `String`, `Class`, an enum, an annotation, or an array of \
                     those can be one",
                ),
            },
        }
    };
    if let Some(message) = problem {
        out.push(CheckId::InvalidAnnotationElementType.at(type_node, message));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import};
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

    fn class_of(flags: ClassFlags) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".into()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags,
        }
    }

    fn resolver() -> MapResolver {
        let members = [
            ("java/lang/String", class_of(ClassFlags::default())),
            ("java/lang/Class", class_of(ClassFlags::default())),
            ("java/lang/Object", class_of(ClassFlags::default())),
            ("java/util/List", class_of(ClassFlags { is_interface: true, ..ClassFlags::default() })),
            ("com/acme/MyObj", class_of(ClassFlags::default())),
            ("com/acme/Color", class_of(ClassFlags { is_enum: true, ..ClassFlags::default() })),
            (
                "com/acme/Sub",
                class_of(ClassFlags {
                    is_annotation: true,
                    is_interface: true,
                    ..ClassFlags::default()
                }),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let simple = [
            ("String", "java/lang/String"),
            ("Class", "java/lang/Class"),
            ("Object", "java/lang/Object"),
            ("List", "java/util/List"),
            ("MyObj", "com/acme/MyObj"),
            ("Color", "com/acme/Color"),
            ("Sub", "com/acme/Sub"),
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
        annotation_decl_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    /// The whole legal set (JLS §9.6.1), in one declaration — none of it may be flagged.
    #[test]
    fn every_legal_element_type_is_fine() {
        let src = r#"@interface A {
            String s();
            int i();
            boolean b();
            Class<?> k();
            Class<? extends Number> k2();
            Color c();
            Sub sub();
            String[] a();
            int[] ia();
            Color[] ca();
        }"#;
        assert!(codes(src).is_empty(), "{:?}", codes(src));
    }

    /// The report this check comes from: an element typed as a class of your own reads fine, and
    /// then every USE of it looks like the mistake.
    #[test]
    fn a_class_typed_element_is_flagged() {
        assert_eq!(codes(r#"@interface A { MyObj o(); }"#), ["invalid-annotation-element-type"]);
        assert_eq!(codes(r#"@interface A { MyObj[] o(); }"#), ["invalid-annotation-element-type"]);
        assert_eq!(codes(r#"@interface A { Object o(); }"#), ["invalid-annotation-element-type"]);
    }

    #[test]
    fn a_generic_element_is_flagged() {
        assert_eq!(
            codes(r#"@interface A { List<String> l(); }"#),
            ["invalid-annotation-element-type"]
        );
    }

    /// An array of arrays is not in the set — only a one-dimensional one is.
    #[test]
    fn a_two_dimensional_array_element_is_flagged() {
        assert_eq!(
            codes(r#"@interface A { String[][] a(); }"#),
            ["invalid-annotation-element-type"]
        );
    }

    /// `String a()[]` spells the same array with the dimension after the name, so the two spellings
    /// have to be counted together or the limit is one that can be walked around.
    #[test]
    fn a_trailing_dimension_counts_towards_the_same_limit() {
        assert!(codes(r#"@interface A { String a()[]; }"#).is_empty());
        assert_eq!(
            codes(r#"@interface A { String[] a()[]; }"#),
            ["invalid-annotation-element-type"]
        );
    }

    #[test]
    fn a_void_element_is_flagged() {
        assert_eq!(codes(r#"@interface A { void v(); }"#), ["invalid-annotation-element-type"]);
    }

    /// A type that does not resolve says nothing about which family it belongs to — and a name we
    /// cannot place is the everyday state of a project whose jars are not indexed.
    #[test]
    fn an_unresolved_element_type_is_left_alone() {
        assert!(codes(r#"@interface A { com.unknown.Thing t(); }"#).is_empty());
        assert!(codes(r#"@interface A { Whatever w(); }"#).is_empty());
    }

    /// A method of an ordinary interface is not an annotation element and is never judged here.
    #[test]
    fn an_ordinary_interface_method_is_not_an_element() {
        assert!(codes(r#"interface A { List<String> l(); MyObj o(); }"#).is_empty());
    }
}
