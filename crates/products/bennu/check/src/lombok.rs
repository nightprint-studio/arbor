//! The tree-sitter side of [`bennu_lombok`].
//!
//! The catalogue and the import gate live in their own dependency-free crate, because `bennu-intel`
//! asks them the same questions from a completely different representation. What is left here is the
//! adaptation: pulling an `import …;` and an `@Annotation(…)` out of the CST and handing the strings
//! over. Every check in this crate that has to stay quiet about a Lombok-generated member goes
//! through this module — so none of them can carry its own idea of what `@Builder` does, which is how
//! the blank-final check came to report the finals of a `@Builder` class as never initialised.

use bennu_lombok::prelude::*;
use tree_sitter::Node;

/// The `import …;` declarations of a file, parsed. Built once per check run from the shared node
/// slice and passed down as a slice — parsing is cheap, but the gate is asked per type declaration.
pub(crate) fn imports_from_nodes(nodes: &[Node], bytes: &[u8]) -> Vec<ParsedImport> {
    nodes
        .iter()
        .filter(|n| n.kind() == "import_declaration")
        .filter_map(|n| n.utf8_text(bytes).ok())
        .filter_map(parse_import_declaration)
        .collect()
}

/// Same, from a compilation-unit root — for the checks that hold a tree rather than the shared node
/// slice. Imports are top-level children, so this never descends into a body.
pub(crate) fn imports_from_root(root: Node, bytes: &[u8]) -> Vec<ParsedImport> {
    let mut out = Vec::new();
    let mut c = root.walk();
    for ch in root.children(&mut c) {
        if ch.kind() != "import_declaration" {
            continue;
        }
        if let Some(i) = ch.utf8_text(bytes).ok().and_then(parse_import_declaration) {
            out.push(i);
        }
    }
    out
}

/// Whether the file imports Lombok at all — the coarse capability gate. A bare `@Data` with no
/// Lombok import is the project's OWN annotation: it generates nothing, so it silences nothing.
pub(crate) fn file_uses_lombok(imports: &[ParsedImport]) -> bool {
    bennu_lombok::imports::file_uses_lombok(imports.iter().map(ParsedImport::as_path))
}

/// Whether `keyword` (`"val"` / `"var"`) is Lombok's inference keyword in this file rather than an
/// ordinary type name.
pub(crate) fn imports_keyword(keyword: &str, imports: &[ParsedImport]) -> bool {
    bennu_lombok::imports::imports_keyword(keyword, imports.iter().map(ParsedImport::as_path))
}

/// An annotation written on a declaration.
pub(crate) struct AnnotationRef<'a> {
    /// The name as written — `Data`, `lombok.Data`, `lombok.experimental.SuperBuilder`.
    pub written: &'a str,
    /// Its last segment, so `@Data` and `@lombok.Data` read the same.
    pub simple: &'a str,
    /// The text of its arguments, parentheses and all; `None` for a marker annotation. Kept verbatim
    /// because the only thing read out of them is whether one boolean flag is set, and
    /// [`flag_is_true`] compacts whitespace itself.
    pub args: Option<&'a str>,
}

impl AnnotationRef<'_> {
    /// Whether this annotation resolves to Lombok: the file imports it, or it is written
    /// fully-qualified (`@lombok.Data`), the one spelling that needs no import.
    fn resolves_to_lombok(&self, file_imports_lombok: bool) -> bool {
        file_imports_lombok || ImportPath::new(self.written, false).is_lombok()
    }
}

/// The annotations in a declaration's `modifiers` node — where every annotation on a type, method or
/// field sits, before the `class`/`enum` keyword.
pub(crate) fn annotations_of<'a>(decl: Node<'a>, bytes: &'a [u8]) -> Vec<AnnotationRef<'a>> {
    let mut out = Vec::new();
    let mut c = decl.walk();
    for ch in decl.children(&mut c) {
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for a in ch.children(&mut mc) {
            if !matches!(a.kind(), "marker_annotation" | "annotation") {
                continue;
            }
            let Some(written) = a.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok())
            else {
                continue;
            };
            out.push(AnnotationRef {
                written,
                simple: written.rsplit('.').next().unwrap_or(written),
                args: a.child_by_field_name("arguments").and_then(|n| n.utf8_text(bytes).ok()),
            });
        }
    }
    out
}

/// Whether `decl` carries a Lombok annotation for which `wanted` holds — with the capability gate
/// applied once, here, in the one form that is right for every caller.
pub(crate) fn has_lombok_annotation(
    decl: Node,
    bytes: &[u8],
    imports: &[ParsedImport],
    wanted: impl Fn(&AnnotationRef) -> bool,
) -> bool {
    let present = file_uses_lombok(imports);
    annotations_of(decl, bytes)
        .iter()
        .any(|a| wanted(a) && a.resolves_to_lombok(present))
}
