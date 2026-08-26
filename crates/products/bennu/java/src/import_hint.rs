//! `import_hint` — detect the simple TYPE name under the caret that is used but not imported.
//!
//! The detection half of the "Import class" intention: given `(source, caret)`, return the simple
//! type name at the caret when it's a bare type usage (`type_identifier`) that ISN'T already imported
//! and ISN'T a type declared in this file (nor a type variable). Candidate-FQN lookup and the
//! java.lang / same-package / star-import filtering happen in the resolver-backed layer that knows the
//! classpath and the file's package — this stays a pure, tree-sitter-only signal.


use crate::symbols::{extract_symbols, node_text};

/// The simple type name under the caret that needs an import, or `None` when the caret isn't on such a
/// name. Conservative: only a bare `type_identifier` (not a qualified `Outer.Inner`), not a type
/// variable, not already imported, and not a type declared in this file.
pub fn simple_type_needing_import(source: &str, offset: usize) -> Option<String> {
    let tree = crate::grammar::parse_java(source)?;
    let root = tree.root_node();

    let node = root.named_descendant_for_byte_range(offset, offset)?;
    // Only a simple type reference. A `class Foo` declaration's name is an `identifier` (not a
    // `type_identifier`), so declarations never match — we only ever fire on a type USAGE.
    if node.kind() != "type_identifier" {
        return None;
    }
    // A qualified name (`Outer.Inner`, `pkg.Type`) has the inner part as a `type_identifier` under a
    // `scoped_type_identifier` — it's already qualified, so no import is needed.
    if node.parent().map(|p| p.kind()) == Some("scoped_type_identifier") {
        return None;
    }
    let simple = node_text(&node, source.as_bytes())?;
    // A type variable (`T`, `E`, `T1`) is not importable.
    if looks_like_type_variable(&simple) {
        return None;
    }

    let symbols = extract_symbols(source);
    // Already brought in by a specific import (`import x.y.Simple;`).
    if symbols
        .imports
        .iter()
        .any(|i| i.simple_name().as_deref() == Some(simple.as_str()))
    {
        return None;
    }
    // A type declared in THIS file (a top-level or nested sibling type) needs no import.
    if symbols.types.iter().any(|t| t.name == simple) {
        return None;
    }
    Some(simple)
}

/// A conventional type-variable name: a single uppercase letter, or one uppercase letter followed by
/// digits (`T`, `E`, `K`, `T1`, `T2`) — never a real, importable class name.
fn looks_like_type_variable(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase() && chars.all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offset of the first occurrence of `needle` in `src`, plus 1 (so the caret sits INSIDE the token).
    fn caret(src: &str, needle: &str) -> usize {
        src.find(needle).expect("needle present") + 1
    }

    #[test]
    fn bare_unimported_type_is_detected() {
        let src = "package a;\nclass C { void m() { List x = null; } }";
        assert_eq!(
            simple_type_needing_import(src, caret(src, "List x")).as_deref(),
            Some("List")
        );
    }

    #[test]
    fn already_imported_type_is_not_detected() {
        let src = "package a;\nimport java.util.List;\nclass C { void m() { List x = null; } }";
        assert_eq!(simple_type_needing_import(src, caret(src, "List x")), None);
    }

    #[test]
    fn type_declared_in_this_file_is_not_detected() {
        let src = "package a;\nclass Helper {}\nclass C { void m() { Helper h = null; } }";
        assert_eq!(
            simple_type_needing_import(src, caret(src, "Helper h")),
            None
        );
    }

    #[test]
    fn qualified_name_is_not_detected() {
        let src = "package a;\nclass C { java.util.List x; }";
        // The caret on the qualified `List` (a scoped_type_identifier) → no bare-import offer.
        assert_eq!(simple_type_needing_import(src, caret(src, "List x")), None);
    }

    #[test]
    fn type_variable_is_not_detected() {
        let src = "package a;\nclass C<T> { T value; }";
        assert_eq!(simple_type_needing_import(src, caret(src, "T value")), None);
    }

    #[test]
    fn caret_on_a_non_type_is_not_detected() {
        let src = "package a;\nclass C { int count; void m() { count = 1; } }";
        // On the `count` identifier (a variable, not a type) → nothing.
        assert_eq!(
            simple_type_needing_import(src, caret(src, "count = 1")),
            None
        );
    }
}
