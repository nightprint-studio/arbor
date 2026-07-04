//! Unresolved-type diagnostics — a simple type name written in a type position (`Fooo x;`,
//! `extends Barr`, `List<Bazz>`, `catch (Quxx e)`) that resolves to nothing. Catches the classic
//! typo'd class name before javac does.
//!
//! This is the most false-positive-prone check, so the gate is deliberately tight (docs: never a
//! false "cannot resolve"):
//!   * runs only when `jdk_available` — otherwise `java.lang` / library types wouldn't resolve and
//!     everything would look unknown;
//!   * only **simple** (unqualified) names — a qualified `a.b.C` (`scoped_type_identifier`) is left
//!     alone (we don't second-guess a written FQN);
//!   * excluded up front: in-scope **type parameters** (`<T>`), types **declared in this file**,
//!     `var`, and the ubiquitous `java.lang` names;
//!   * flagged only when the resolver — imports, project index, star-imports, `java.lang` — returns
//!     nothing.

use std::collections::HashSet;

use bennu_java::prelude::{extract_symbols, FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

/// The bare `java.lang` names an unqualified program may use without importing — always resolvable,
/// so never flag them even if a minimal resolver doesn't seed them.
const JAVA_LANG: &[&str] = &[
    "String", "Object", "Integer", "Long", "Boolean", "Double", "Float", "Character", "Byte",
    "Short", "Number", "CharSequence", "Iterable", "Comparable", "Runnable", "Thread", "Class",
    "Exception", "Throwable", "Error", "RuntimeException", "Void", "Math", "System", "Enum",
    "Cloneable", "Comparable", "Deprecated", "Override", "SuppressWarnings", "SafeVarargs",
    "FunctionalInterface", "Iterable", "StringBuilder", "StringBuffer", "Appendable", "AutoCloseable",
];

/// Parse `source` and flag unresolved simple type names.
pub fn unresolved_types(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    unresolved_types_in(tree.root_node(), source, &symbols, resolver)
}

/// Tree-driven core: reuses the caller's `symbols` (no re-extraction per validation pass).
pub fn unresolved_types_in(
    root: Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();

    // Names that are always resolvable in this file: declared types (incl. nested) + type params.
    let mut known: HashSet<String> = symbols.types.iter().map(|t| t.name.clone()).collect();
    collect_type_params(root, bytes, &mut known);

    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        if n.kind() != "type_identifier" {
            continue;
        }
        // Qualified name segment (`a.b.C`) or a *declared* type-parameter name → not a use we judge.
        if matches!(n.parent().map(|p| p.kind()), Some("scoped_type_identifier") | Some("type_parameter")) {
            continue;
        }
        let Ok(name) = n.utf8_text(bytes) else { continue };
        if name == "var" || known.contains(name) || JAVA_LANG.contains(&name) {
            continue;
        }
        if resolver.resolve_simple_name(name, &symbols.imports).is_some() {
            continue;
        }
        out.push(Diagnostic {
            message: format!("Cannot resolve symbol `{name}`"),
            severity: "error".to_string(),
            start: n.start_byte(),
            end: n.end_byte(),
        });
    }
    out
}

/// Gather every type-parameter name declared anywhere in the file (`<T>`, `<K, V>`, `<T extends X>`).
/// Collected file-wide (not per-scope) — over-including is conservative: a name that's a type
/// parameter somewhere is never flagged as an unresolved type.
fn collect_type_params(root: Node, bytes: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
        if n.kind() == "type_parameter" {
            // The param name is the first `type_identifier` child.
            let mut tc = n.walk();
            for ch in n.named_children(&mut tc) {
                if ch.kind() == "type_identifier" {
                    if let Ok(t) = ch.utf8_text(bytes) {
                        out.insert(t.to_string());
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, _binary: &str) -> Option<Arc<ClassMembers>> {
            None
        }
        fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String> {
            if let Some(b) = self.simple.get(name) {
                return Some(b.clone());
            }
            // Honour single-type imports the way the real resolver does.
            imports.iter().find_map(|i| {
                (i.simple_name() == Some(name)).then(|| i.path.replace('.', "/"))
            })
        }
    }

    fn resolver() -> MapResolver {
        let simple = [("Widget", "com/acme/Widget"), ("Gadget", "com/acme/Gadget")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { simple }
    }

    fn diags(src: &str) -> Vec<String> {
        unresolved_types(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn resolvable_type_is_ok() {
        assert!(diags("class C { Widget w; }").is_empty());
    }

    #[test]
    fn unknown_type_is_flagged() {
        let d = diags("class C { Wodget w; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Wodget"), "{d:?}");
    }

    #[test]
    fn java_lang_is_never_flagged() {
        assert!(diags("class C { String s; Object o; Exception e; }").is_empty());
    }

    #[test]
    fn same_file_type_is_ok() {
        // `Helper` is declared in the file → resolvable even though the resolver doesn't know it.
        assert!(diags("class C { Helper h; } class Helper {}").is_empty());
    }

    #[test]
    fn type_parameter_is_not_flagged() {
        // `T` is a type parameter, not an unresolved type.
        assert!(diags("class Box<T> { T value; T get() { return value; } }").is_empty());
    }

    #[test]
    fn method_type_parameter_is_not_flagged() {
        assert!(diags("class C { <R> R pick(R a, R b) { return a; } }").is_empty());
    }

    #[test]
    fn qualified_name_is_left_alone() {
        // A written FQN is not second-guessed (no false positive on `com.acme.Whatever`).
        assert!(diags("class C { com.acme.Whatever w; }").is_empty());
    }

    #[test]
    fn imported_type_is_ok() {
        assert!(diags("import java.util.List;\nclass C { List xs; }").is_empty());
    }

    #[test]
    fn unknown_in_extends_is_flagged() {
        let d = diags("class C extends Nonesuch {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Nonesuch"), "{d:?}");
    }

    #[test]
    fn unknown_in_generics_and_catch_is_flagged() {
        assert_eq!(diags("class C { java.util.List<Bogus> xs; }").len(), 1);
        let d = diags("class C { void m() { try {} catch (Ouch e) {} } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Ouch"), "{d:?}");
    }
}
