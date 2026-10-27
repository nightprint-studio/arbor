//! A package-private class used from another package (`not.def.public.cant.access`).
//!
//! `class Hidden { }` with no `public` is visible inside its own package only; naming it from anywhere
//! else — in a type position, after `new`, in an `import` — does not compile. The member-access sibling
//! of this lives in `hierarchy/visibility.rs`; this is the type half.
//!
//! Judged only for the project's own top-level types, and only when the index says the type is
//! package-private (a type written before the index recorded that is simply not judged): a library's
//! internal classes, a nested type whose modifiers the index does not keep, and a file whose package is
//! unknown are all silent.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;

/// Every use of a package-private type from outside its package.
pub fn type_access_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let Some(package) = symbols.package.as_deref().filter(|p| !p.is_empty()) else { return Vec::new() };
    let package = package.replace('.', "/");
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        let binary = match n.kind() {
            // The head of `a.b.C` is a `type_identifier` too; the whole scoped name is judged instead.
            "type_identifier" if n.parent().is_some_and(|p| p.kind() == "scoped_type_identifier") => continue,
            "type_identifier" | "scoped_type_identifier" => n
                .utf8_text(bytes)
                .ok()
                .and_then(|w| crate::support::resolve::type_binary_at(w, n, bytes, symbols, resolver)),
            "import_declaration" => imported_binary(n, bytes),
            _ => None,
        };
        let Some(binary) = binary else { continue };
        if let Some(owner) = inaccessible_from(&binary, &package, resolver) {
            let simple = binary.rsplit('/').next().unwrap_or(&binary);
            out.push(CheckId::InaccessibleMember.at(
                n,
                format!("`{simple}` is not public in `{}`; it cannot be accessed from outside its package", owner.replace('/', ".")),
            ));
        }
    }
    out
}

/// The package of `binary` when it is a project top-level type recorded as package-private and that
/// package is not `from`.
fn inaccessible_from(binary: &str, from: &str, resolver: &dyn TypeResolver) -> Option<String> {
    if binary.contains('$') || !resolver.is_project_type(binary) {
        return None;
    }
    let (package, _) = binary.rsplit_once('/')?;
    if package == from {
        return None;
    }
    resolver.members_of(binary).filter(|cm| cm.flags.is_package_private)?;
    Some(package.to_string())
}

/// `import a.b.C;` as `a/b/C` — a single-type import only.
fn imported_binary(import: Node, bytes: &[u8]) -> Option<String> {
    let text = import.utf8_text(bytes).ok()?;
    let body = text.trim().strip_prefix("import")?.trim().trim_end_matches(';').trim();
    if body.starts_with("static") || body.ends_with('*') {
        return None;
    }
    Some(body.replace('.', "/"))
}
