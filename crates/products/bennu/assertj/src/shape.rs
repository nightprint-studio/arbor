//! What a declared type is, in the only terms a rewrite cares about: which AssertJ assertion object
//! `assertThat` returns for it, and so which checks exist on it.
//!
//! A closed list of JDK types, recognised only when the file really means the JDK's: a project's own
//! `List` imported from `com.acme` is not `java.util.List`, and `assertThat(list).isEmpty()` would
//! not compile against it.

use tree_sitter::Node;

use crate::resolve::names_type;
use crate::scope::Decl;
use crate::syntax::children;
use crate::unit::Unit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `java.lang.String`.
    Str,
    /// `CharSequence` / `StringBuilder` — the text types without `contains` or `startsWith`.
    TextLike,
    /// A `java.util` collection. `element` is its single written type argument, when it has a plain one.
    Collection { element: Option<String> },
    Map,
    Optional,
    Array,
    Unknown,
}

const COLLECTIONS: &[&str] = &["List", "ArrayList", "LinkedList", "Set", "HashSet", "TreeSet", "Collection"];
const MAPS: &[&str] = &["Map", "HashMap", "TreeMap", "LinkedHashMap"];

pub(crate) fn shape_of(unit: &Unit<'_>, decl: &Decl<'_>) -> Shape {
    let Some(ty) = decl.ty else { return Shape::Unknown };
    if decl.dims > 0 || ty.kind() == "array_type" {
        return Shape::Array;
    }
    let (base, arguments) = match ty.kind() {
        "type_identifier" | "scoped_type_identifier" => (ty, None),
        "generic_type" => {
            let parts = children(ty);
            let Some(base) = parts.first().copied() else { return Shape::Unknown };
            (base, parts.iter().copied().find(|p| p.kind() == "type_arguments"))
        }
        _ => return Shape::Unknown,
    };
    let written = unit.compact(base);
    let simple = written.rsplit('.').next().unwrap_or_default();
    let is = |package: &str| names_type(&written, package, simple, &unit.facts);
    match simple {
        "String" if arguments.is_none() && is("java.lang") => Shape::Str,
        "CharSequence" | "StringBuilder" if arguments.is_none() && is("java.lang") => Shape::TextLike,
        s if COLLECTIONS.contains(&s) && is("java.util") => {
            Shape::Collection { element: arguments.and_then(|a| sole_argument(unit, a)) }
        }
        s if MAPS.contains(&s) && is("java.util") => Shape::Map,
        "Optional" if is("java.util") => Shape::Optional,
        _ => Shape::Unknown,
    }
}

/// The type argument of `List<String>`; `None` for a wildcard, a nested generic, or several.
fn sole_argument(unit: &Unit<'_>, arguments: Node<'_>) -> Option<String> {
    match children(arguments).as_slice() {
        [one] if matches!(one.kind(), "type_identifier" | "scoped_type_identifier") => Some(unit.compact(*one)),
        _ => None,
    }
}
