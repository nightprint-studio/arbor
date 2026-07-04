//! Unused-import diagnostics — a single-type `import a.b.C;` whose simple name never appears
//! anywhere else in the file (identifiers *or* comments) → a `warning`.
//!
//! Deliberately conservative to never produce a false "unused":
//!   * only plain single-type imports are checked — `import static …` and wildcard `import a.b.*`
//!     are skipped (their usage is a member name / an open set, not this one simple name);
//!   * a name that appears as ANY identifier outside the imports counts as used (even if it's an
//!     unrelated local of the same name — better a missed unused than a wrong one);
//!   * a name appearing in a comment (a Javadoc `{@link C}`) counts as used, so imports kept for
//!     documentation aren't flagged.

use std::collections::HashSet;

use bennu_java::prelude::TypeResolver;
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag a single-type `import a.b.C;` whose type the resolver can't find — the classic typo or a
/// class that no longer exists (mirrors an IDE's red import). Resolver-backed, so it runs only when
/// the JDK/classpath is available; `static` and wildcard imports are skipped (not one resolvable
/// type). Conservative on nested types: `a.b.Outer.Inner` is tried as `a/b/Outer/Inner`,
/// `a/b/Outer$Inner`, … so a valid inner-class import is never mis-flagged.
pub fn unresolved_imports(root: Node, source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() != "import_declaration" {
            continue;
        }
        let Ok(text) = child.utf8_text(bytes) else { continue };
        // Skip `import static …` and wildcard `import a.b.*;` — not a single resolvable type.
        if text.contains("static") || text.trim_end_matches(';').trim_end().ends_with('*') {
            continue;
        }
        let Some(name_node) = dotted_name(child) else { continue };
        let Ok(dotted) = name_node.utf8_text(bytes) else { continue };
        if !resolves_import(dotted, resolver) {
            out.push(Diagnostic {
                message: format!("Cannot resolve import `{dotted}`"),
                severity: "error".to_string(),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
        }
    }
    out
}

/// The dotted type path node of an import (`scoped_identifier` / `identifier`).
fn dotted_name(import: Node) -> Option<Node> {
    let mut c = import.walk();
    for ch in import.named_children(&mut c) {
        if matches!(ch.kind(), "scoped_identifier" | "identifier") {
            return Some(ch);
        }
    }
    None
}

/// Whether a dotted import path resolves to a known class, trying each package/inner split so a
/// nested type (`a.b.Outer.Inner` → `a/b/Outer$Inner`) resolves. Unknown at every split → `false`.
fn resolves_import(dotted: &str, resolver: &dyn TypeResolver) -> bool {
    let segs: Vec<&str> = dotted.split('.').collect();
    if segs.len() < 2 {
        return true; // a bare name import is unusual; don't second-guess it
    }
    // Split point k: first k segments are the package (`/`-joined), the rest the (possibly nested)
    // class (`$`-joined). Start from the deepest package (plain `a/b/C`) inward.
    for k in (1..segs.len()).rev() {
        let binary = format!("{}/{}", segs[..k].join("/"), segs[k..].join("$"));
        if resolver.members_of(&binary).is_some() {
            return true;
        }
    }
    false
}

/// Flag a `import a.b.C;` that repeats an import already declared above it (a `warning`). Matched by
/// the exact import text (`static` and wildcard forms included), so only true duplicates are flagged.
pub fn duplicate_imports(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() != "import_declaration" {
            continue;
        }
        let Ok(text) = child.utf8_text(bytes) else { continue };
        // Normalise on the import path (drop the trailing `;` and inner whitespace).
        let key: String = text.trim_end_matches(';').split_whitespace().collect();
        if !seen.insert(key) {
            out.push(Diagnostic {
                message: "Duplicate import".to_string(),
                severity: "warning".to_string(),
                start: child.start_byte(),
                end: child.end_byte(),
            });
        }
    }
    out
}

/// Every unused single-type import in `root`, as `warning` diagnostics spanning the whole
/// `import …;` statement.
pub fn unused_imports(root: Node, source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let imports = collect_imports(root, bytes);
    if imports.is_empty() {
        return Vec::new();
    }
    let used = collect_used_idents(root, bytes);
    let comments = collect_comment_text(root, bytes);

    let mut out = Vec::new();
    for imp in imports {
        if used.contains(&imp.simple) || word_in(&comments, &imp.simple) {
            continue;
        }
        out.push(Diagnostic {
            message: format!("Unused import `{}`", imp.simple),
            severity: "warning".to_string(),
            start: imp.start,
            end: imp.end,
        });
    }
    out
}

/// A single-type import: the imported simple name + the `import …;` statement span.
struct ImportDecl {
    simple: String,
    start: usize,
    end: usize,
}

/// Collect the plain single-type imports (skipping `static` + wildcard).
fn collect_imports(root: Node, bytes: &[u8]) -> Vec<ImportDecl> {
    let mut out = Vec::new();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        if child.kind() != "import_declaration" {
            continue;
        }
        // Wildcard fallback via text (belt-and-suspenders around the `asterisk` node).
        let is_wildcard_text = child.utf8_text(bytes).map(|t| t.contains(".*")).unwrap_or(false);

        let mut is_static = false;
        let mut is_wildcard = is_wildcard_text;
        let mut simple: Option<String> = None;
        let mut cc = child.walk();
        for part in child.children(&mut cc) {
            match part.kind() {
                "static" => is_static = true,
                "asterisk" => is_wildcard = true,
                "scoped_identifier" => {
                    if let Some(name) = part.child_by_field_name("name") {
                        simple = name.utf8_text(bytes).ok().map(str::to_string);
                    }
                }
                "identifier" => {
                    simple = part.utf8_text(bytes).ok().map(str::to_string);
                }
                _ => {}
            }
        }
        if is_static || is_wildcard {
            continue;
        }
        if let Some(simple) = simple {
            out.push(ImportDecl { simple, start: child.start_byte(), end: child.end_byte() });
        }
    }
    out
}

/// Every identifier / type-identifier used OUTSIDE the import (and package) declarations.
fn collect_used_idents(root: Node, bytes: &[u8]) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        // The import statements themselves are where the names are *declared for use*, not a use;
        // the package declaration is never a type reference.
        if matches!(n.kind(), "import_declaration" | "package_declaration") {
            continue;
        }
        if matches!(n.kind(), "identifier" | "type_identifier") {
            if let Ok(t) = n.utf8_text(bytes) {
                set.insert(t.to_string());
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    set
}

/// All comment text in the file, concatenated — so an import referenced only in a Javadoc
/// (`{@link Foo}`) still counts as used.
fn collect_comment_text(root: Node, bytes: &[u8]) -> String {
    let mut s = String::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "line_comment" | "block_comment") {
            if let Ok(t) = n.utf8_text(bytes) {
                s.push_str(t);
                s.push(' ');
            }
            continue;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    s
}

/// A whole-word occurrence of `name` in `haystack` (so `List` doesn't match `ListView`).
fn word_in(haystack: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let boundary = |c: Option<char>| c.map_or(true, |c| !c.is_alphanumeric() && c != '_');
    haystack.match_indices(name).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + name.len()..].chars().next();
        boundary(before) && boundary(after)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn unused(src: &str) -> Vec<String> {
        let tree = parse(src);
        unused_imports(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    // A resolver that knows a fixed set of binary names (incl. an inner class).
    struct Idx(std::collections::HashSet<String>);
    impl TypeResolver for Idx {
        fn members_of(&self, binary: &str) -> Option<std::sync::Arc<bennu_java::prelude::ClassMembers>> {
            self.0.contains(binary).then(|| {
                std::sync::Arc::new(bennu_java::prelude::ClassMembers {
                    superclass: None,
                    interfaces: Vec::new(),
                    methods: Vec::new(),
                    fields: Vec::new(),
                    flags: Default::default(),
                })
            })
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[bennu_java::prelude::Import]) -> Option<String> {
            None
        }
    }

    fn unresolved(src: &str, known: &[&str]) -> Vec<String> {
        let tree = parse(src);
        let idx = Idx(known.iter().map(|s| s.to_string()).collect());
        unresolved_imports(tree.root_node(), src, &idx).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn resolvable_import_is_ok() {
        assert!(unresolved("import java.util.List;\nclass C {}", &["java/util/List"]).is_empty());
    }

    #[test]
    fn unknown_import_is_flagged() {
        let d = unresolved("import com.acme.Nope;\nclass C {}", &["java/util/List"]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("com.acme.Nope"), "{d:?}");
    }

    #[test]
    fn static_and_wildcard_imports_are_not_resolution_checked() {
        let src = "import static java.lang.Math.max;\nimport com.acme.*;\nclass C {}";
        assert!(unresolved(src, &[]).is_empty());
    }

    #[test]
    fn inner_class_import_resolves_via_dollar() {
        // `a.b.Outer.Inner` must be tried as `a/b/Outer$Inner`.
        let src = "import a.b.Outer.Inner;\nclass C {}";
        assert!(unresolved(src, &["a/b/Outer$Inner"]).is_empty());
    }

    #[test]
    fn used_import_is_not_flagged() {
        let src = "package a;\nimport java.util.List;\nclass Foo { List<String> xs; }\n";
        assert!(unused(src).is_empty());
    }

    #[test]
    fn unused_import_is_flagged() {
        let src = "package a;\nimport java.util.List;\nclass Foo {}\n";
        let u = unused(src);
        assert_eq!(u.len(), 1);
        assert!(u[0].contains("List"), "{:?}", u);
    }

    #[test]
    fn import_used_only_in_javadoc_is_not_flagged() {
        let src = "package a;\nimport java.util.Map;\n/** See {@link Map} for details. */\nclass Foo {}\n";
        assert!(unused(src).is_empty(), "an import used in a comment must not be flagged");
    }

    #[test]
    fn static_and_wildcard_imports_are_skipped() {
        let src = "package a;\nimport static java.util.Collections.emptyList;\nimport java.util.*;\nclass Foo {}\n";
        assert!(unused(src).is_empty(), "static + wildcard imports are never flagged");
    }

    #[test]
    fn word_boundary_prevents_partial_match() {
        // `List` is imported and unused; `ListView` is a different type used in the body. The
        // partial match must NOT count `List` as used.
        let src = "package a;\nimport java.util.List;\nclass Foo { ListView v; }\n";
        let u = unused(src);
        assert_eq!(u.len(), 1);
        assert!(u[0].contains("List"));
    }

    #[test]
    fn multiple_imports_mixed() {
        let src = "package a;\nimport java.util.List;\nimport java.util.Map;\nclass Foo { Map<String,String> m; }\n";
        let u = unused(src);
        assert_eq!(u.len(), 1);
        assert!(u[0].contains("List"), "only List is unused ({:?})", u);
    }

    fn dups(src: &str) -> usize {
        let tree = parse(src);
        duplicate_imports(tree.root_node(), src).len()
    }

    #[test]
    fn duplicate_import_is_flagged_once() {
        let src = "package a;\nimport java.util.List;\nimport java.util.List;\nclass Foo { List<String> xs; }\n";
        assert_eq!(dups(src), 1, "the second identical import is the duplicate");
    }

    #[test]
    fn distinct_imports_are_not_duplicates() {
        let src = "package a;\nimport java.util.List;\nimport java.util.Map;\nclass Foo {}\n";
        assert_eq!(dups(src), 0);
    }
}
