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
    let nodes = crate::check::collect_nodes(tree.root_node());
    unresolved_types_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn unresolved_types_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();

    // Names that are always resolvable in this file: declared types (incl. nested) + type params.
    let mut known: HashSet<String> = symbols.types.iter().map(|t| t.name.clone()).collect();
    collect_type_params(nodes, bytes, &mut known);

    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "type_identifier" {
            continue;
        }
        // Qualified name segment (`a.b.C`) or a *declared* type-parameter name → not a use we judge.
        if matches!(n.parent().map(|p| p.kind()), Some("scoped_type_identifier") | Some("type_parameter")) {
            continue;
        }
        // The qualifier of a method reference (`x::method`) — tree-sitter parses the LHS as a
        // `type_identifier` even when it's actually a VARIABLE / field / parameter (`list::add`,
        // `helper::process`), which it can't disambiguate from a class (`Integer::parseInt`). Since a
        // method-ref qualifier can legitimately be a value, never flag it as an unresolved type (only
        // a genuine `TypoClass::method` slips through — the conservative trade for zero false positives).
        if n.parent().map(|p| p.kind()) == Some("method_reference") {
            continue;
        }
        let Ok(name) = n.utf8_text(bytes) else { continue };
        if name == "var" || known.contains(name) || JAVA_LANG.contains(&name) {
            continue;
        }
        // Lombok `val` (an inferred `final` local) parses as a `type_identifier` named `val`. When
        // it's the type of a local declaration AND the file imports Lombok's `val` (so it's really the
        // inference keyword, not a class), skip it like `var` — `val x = repo.find();` isn't flagged
        // "Cannot resolve symbol `val`". Without the import, `val` isn't Lombok's and IS a real
        // unresolved type, so it stays flagged.
        if name == "val"
            && n.parent().map(|p| p.kind()) == Some("local_variable_declaration")
            && imports_lombok_val(symbols)
        {
            continue;
        }
        // Resolvable via imports, the file's OWN package (no import needed), or the global lookup.
        // Uses the shared `type_binary` so a bare same-package type (`C` referencing a sibling class in
        // `com.acme`) resolves to `com/acme/C` instead of being falsely flagged.
        if crate::resolve::type_binary(name, symbols, resolver).is_some() {
            continue;
        }
        out.push(Diagnostic {
            message: format!("Cannot resolve symbol `{name}`"),
            severity: "error".to_string(),
            code: String::new(),
            start: n.start_byte(),
            end: n.end_byte(),
        });
    }
    out
}

/// Whether the file imports Lombok's `val` — the specific `import lombok.val;` or a `lombok.*`
/// wildcard (`val` lives in the core `lombok` package). Only then is a `val`-typed local the Lombok
/// inference keyword rather than an unresolved class named `val`.
fn imports_lombok_val(symbols: &FileSymbols) -> bool {
    symbols.imports.iter().any(|i| {
        if i.star {
            i.path == "lombok"
        } else {
            i.path == "lombok.val"
        }
    })
}

/// Gather every type-parameter name declared anywhere in the file (`<T>`, `<K, V>`, `<T extends X>`).
/// Collected file-wide (not per-scope) — over-including is conservative: a name that's a type
/// parameter somewhere is never flagged as an unresolved type.
fn collect_type_params(nodes: &[Node], bytes: &[u8], out: &mut HashSet<String>) {
    for &n in nodes {
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
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    struct MapResolver {
        simple: HashMap<String, String>,
        /// Binary names for which `members_of` returns a (stub) type — i.e. types that EXIST. The
        /// same-package resolution probes `members_of("<pkg>/<name>")`, so a same-package test seeds
        /// the sibling's binary here.
        known: HashSet<String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.known.contains(binary).then(|| {
                Arc::new(ClassMembers {
                    type_params: Vec::new(),
                    superclass: Some("java/lang/Object".to_string()),
                    interfaces: Vec::new(),
                    methods: Vec::new(),
                    fields: Vec::new(),
                    flags: Default::default(),
                })
            })
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
        MapResolver { simple, known: HashSet::new() }
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
    fn same_package_type_needs_no_import() {
        // `B` (in `com.acme`) references its sibling `Sibling` (also `com.acme`) with NO import — legal
        // in Java, and must NOT be flagged. The flat simple-name index doesn't know `Sibling`, but its
        // exact binary `com/acme/Sibling` resolves (same package).
        let mut r = resolver();
        r.known.insert("com/acme/Sibling".to_string());
        let src = "package com.acme;\nclass B { Sibling s; }";
        let d: Vec<String> = unresolved_types(src, &r).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "a same-package type resolves without an import: {d:?}");

        // A name that is NOT a same-package type (and unknown everywhere) is still flagged.
        let bad = "package com.acme;\nclass B { Nonesuch s; }";
        let d2: Vec<String> = unresolved_types(bad, &r).into_iter().map(|x| x.message).collect();
        assert_eq!(d2.len(), 1, "{d2:?}");
        assert!(d2[0].contains("Nonesuch"), "{d2:?}");
    }

    #[test]
    fn type_parameter_is_not_flagged() {
        // `T` is a type parameter, not an unresolved type.
        assert!(diags("class Box<T> { T value; T get() { return value; } }").is_empty());
    }

    #[test]
    fn var_and_imported_lombok_val_locals_are_not_flagged() {
        // The JDK `var` is always fine; Lombok `val` is fine ONLY when imported → then neither is
        // flagged as an unresolved symbol.
        let src = "import lombok.val;\nclass C { void m() { var x = 1; val y = new Widget(); } }";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
    }

    #[test]
    fn lombok_val_without_import_is_flagged() {
        // No `import lombok.val;` → `val` is not the inference keyword, it's an unresolved type.
        let d = diags("class C { void m() { val y = new Widget(); } }");
        assert!(d.iter().any(|m| m.contains("val")), "unimported val is a real unresolved type: {d:?}");
    }

    #[test]
    fn method_reference_qualifier_is_not_flagged() {
        // `list::add` / `helper::process` — the LHS is a VARIABLE, parsed as a `type_identifier` by
        // tree-sitter, but it must not be flagged as an unresolved type (the false positive the user
        // hit). The declared types here (`Widget`, `String`) resolve, so only the ref qualifier is
        // under test.
        assert!(diags("class C { void m() { Widget c = list::add; } }").is_empty());
        assert!(diags("class C { void m() { String r = helper::process; } }").is_empty());
        assert!(diags("class C { void m() { var f = data::make; } }").is_empty());
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
