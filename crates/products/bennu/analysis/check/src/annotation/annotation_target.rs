//! An annotation written where its own `@Target` does not allow it — javac's
//! `annotation.type.not.applicable`.
//!
//! The pure-AST [`crate::annotation::annotations`] knows three JDK annotations by heart. Every other
//! annotation says where it may go in its `@Target`, which only the resolver can read: out of the
//! class file for a JDK or library type, out of the index for one the project declares.
//!
//! Silent unless the annotation type resolves, its own annotations were read, and its `@Target`
//! names something recognisable. An annotation with no `@Target` at all applies to every declaration,
//! and one allowing `TYPE_USE` applies to every declaration whose type is written — so neither is
//! ever reported.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// The `java.lang.annotation.ElementType` constants.
const ELEMENT_TYPES: &[&str] = &[
    "TYPE", "FIELD", "METHOD", "PARAMETER", "CONSTRUCTOR", "LOCAL_VARIABLE", "ANNOTATION_TYPE", "PACKAGE",
    "TYPE_PARAMETER", "TYPE_USE", "MODULE", "RECORD_COMPONENT",
];

/// The annotations the pure-AST check already judges — reporting them here would say it twice.
const JUDGED_ELSEWHERE: &[&str] =
    &["java/lang/Override", "java/lang/FunctionalInterface", "java/lang/SafeVarargs"];

/// Every annotation placed where its `@Target` forbids.
pub fn annotation_target_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if !matches!(n.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        let Some((wanted, what)) = declaration_context(n) else { continue };
        let Some(name) = n.child_by_field_name("name") else { continue };
        let Ok(written) = name.utf8_text(bytes) else { continue };
        let Some(binary) = crate::support::resolve::type_binary_at(written, n, bytes, symbols, resolver) else {
            continue;
        };
        if JUDGED_ELSEWHERE.contains(&binary.as_str()) {
            continue;
        }
        if !resolver.members_of(&binary).is_some_and(|cm| cm.flags.is_annotation) {
            continue;
        }
        let Some(allowed) = target_of(resolver, &binary) else { continue };
        if allowed.contains(&"TYPE_USE") || wanted.iter().any(|w| allowed.contains(w)) {
            continue;
        }
        let simple = binary.rsplit(['/', '$']).next().unwrap_or(&binary);
        out.push(CheckId::AnnotationNotApplicable.at(
            n,
            format!("`@{simple}` is not applicable to {what}: its `@Target` allows {}", allowed.join(", ")),
        ));
    }
    out
}

/// The `ElementType`s an annotation type's `@Target` lists — `None` when its own annotations were not
/// read, when it has no `@Target` (applicable everywhere), or when the target names nothing known.
fn target_of(resolver: &dyn TypeResolver, binary: &str) -> Option<Vec<&'static str>> {
    let own = resolver.class_annotations(binary);
    let target = own.into_iter().find(|a| a.name == "Target")?;
    let text = target
        .positional
        .first()
        .cloned()
        .or_else(|| target.args.iter().find(|(k, _)| k == "value").map(|(_, v)| v.clone()))?;
    let named: Vec<&'static str> =
        ELEMENT_TYPES.iter().copied().filter(|name| names_constant(&text, name)).collect();
    (!named.is_empty()).then_some(named)
}

/// What the annotation `n` is attached to: the `ElementType`s that admit it there, and how to say it.
fn declaration_context(n: Node) -> Option<(&'static [&'static str], &'static str)> {
    let modifiers = n.parent().filter(|p| p.kind() == "modifiers")?;
    let decl = modifiers.parent()?;
    Some(match decl.kind() {
        "class_declaration" | "interface_declaration" | "enum_declaration" => (&["TYPE"], "a type"),
        "annotation_type_declaration" => (&["TYPE", "ANNOTATION_TYPE"], "an annotation type"),
        "method_declaration" | "annotation_type_element_declaration" => (&["METHOD"], "a method"),
        "constructor_declaration" => (&["CONSTRUCTOR"], "a constructor"),
        "field_declaration" | "constant_declaration" => (&["FIELD"], "a field"),
        "local_variable_declaration" | "resource" | "enhanced_for_statement" => {
            (&["LOCAL_VARIABLE"], "a local variable")
        }
        // A record's header parameter is a component, which several targets reach.
        "formal_parameter" | "spread_parameter"
            if decl.parent().and_then(|p| p.parent()).is_some_and(|g| g.kind() == "record_declaration") =>
        {
            return None
        }
        "formal_parameter" | "spread_parameter" | "catch_formal_parameter" => (&["PARAMETER"], "a parameter"),
        _ => return None,
    })
}

/// Whether `text` names the constant `name` as a whole word — `TYPE` must not match `TYPE_USE`.
fn names_constant(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    text.match_indices(name).any(|(i, _)| {
        let end = i + name.len();
        (i == 0 || !word(bytes[i - 1])) && (end == bytes.len() || !word(bytes[end]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{Annotation, ClassFlags, ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver(HashMap<&'static str, (&'static str, &'static str)>);

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.0.contains_key(binary).then(|| {
                Arc::new(ClassMembers {
                    type_params: Vec::new(),
                    superclass: None,
                    interfaces: Vec::new(),
                    methods: Vec::new(),
                    fields: Vec::new(),
                    flags: ClassFlags { is_annotation: true, is_interface: true, ..ClassFlags::default() },
                })
            })
        }
        fn class_annotations(&self, binary: &str) -> Vec<Annotation> {
            let Some((_, target)) = self.0.get(binary) else { return Vec::new() };
            vec![Annotation {
                name: "Target".into(),
                qualified: "Target".into(),
                start: 0,
                end: 0,
                strings: Vec::new(),
                args: Vec::new(),
                positional: vec![target.to_string()],
            }]
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.0.iter().find(|(_, (simple, _))| *simple == name).map(|(b, _)| b.to_string())
        }
    }

    fn resolver() -> MapResolver {
        MapResolver(
            [
                ("p/MethodOnly", ("MethodOnly", "ElementType.METHOD")),
                ("p/TypeOrField", ("TypeOrField", "{ ElementType.TYPE, ElementType.FIELD }")),
                ("p/Anywhere", ("Anywhere", "{ TYPE_USE }")),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn diags(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let symbols = bennu_java::prelude::extract_symbols(src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        annotation_target_errors_in(&nodes, src, &symbols, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn an_annotation_outside_its_target_is_flagged() {
        assert_eq!(diags("@MethodOnly class C {}").len(), 1);
        assert_eq!(diags("class C { @MethodOnly int f; }").len(), 1);
        assert_eq!(diags("class C { @TypeOrField void m() {} }").len(), 1);
        assert_eq!(diags("class C { void m(@MethodOnly int v) {} }").len(), 1);
        assert_eq!(diags("class C { void m() { @TypeOrField int v = 1; } }").len(), 1);
    }

    #[test]
    fn an_annotation_inside_its_target_is_fine() {
        assert!(diags("class C { @MethodOnly void m() {} }").is_empty());
        assert!(diags("@TypeOrField class C { @TypeOrField int f; }").is_empty());
    }

    #[test]
    fn type_use_admits_every_declaration() {
        assert!(diags("@Anywhere class C { @Anywhere int f; @Anywhere void m(@Anywhere int v) {} }").is_empty());
    }
}
