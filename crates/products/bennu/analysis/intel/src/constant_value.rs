//! The value a hover card shows beside a project constant: its initializer, as written.
//!
//! A library constant's value comes out of its class file (`TypeResolver::constant_value`). A
//! project one has no class file the index reads — its value is the expression after the `=`, and
//! the card already parses the declaring file to find the Javadoc, so it is read from there.
//!
//! Only a **constant** in the sense the reader means it: a `static final` field (or an interface
//! field, which is one without the words) of primitive or `String` type, initialised with an
//! expression made of literals, other constants and operators. A `static final Logger LOG =
//! LoggerFactory.getLogger(…)` is final but not a value anyone wants on a tooltip, and a call or a
//! `new` in the initializer says the same thing about a primitive.

use bennu_java::prelude::{parent_of, parse_java};
use tree_sitter::Node;

use crate::refs::DeclKey;

/// An initializer longer than this is cut with `…`: the card's title is one line of code.
const MAX_CHARS: usize = 120;

/// The types whose `static final` fields are constants a reader wants the value of.
const PRIMITIVE_TYPES: &[&str] = &["integral_type", "floating_point_type", "boolean_type"];

/// The expression kinds a constant initializer is made of. Anything else — a call, a `new`, an
/// array, a lambda — makes the field something other than a value.
const CONSTANT_PARTS: &[&str] = &[
    "decimal_integer_literal", "hex_integer_literal", "octal_integer_literal",
    "binary_integer_literal", "decimal_floating_point_literal", "hex_floating_point_literal",
    "character_literal", "string_literal", "string_fragment", "escape_sequence", "text_block",
    "true", "false", "identifier", "field_access", "scoped_identifier",
    "parenthesized_expression", "unary_expression", "binary_expression", "ternary_expression",
    "cast_expression", "integral_type", "floating_point_type", "boolean_type", "type_identifier",
];

/// The initializer of the project constant `key` names, read from `source` (the file declaring it).
/// `None` for anything that is not a field, not a constant, or not found.
pub(crate) fn project_constant(source: &str, key: &DeclKey) -> Option<String> {
    if !matches!(key, DeclKey::Field { .. }) {
        return None;
    }
    let (start, end) = crate::rename::find_member_name_span(source, key)?;
    let tree = parse_java(source)?;
    let bytes = source.as_bytes();

    let name = tree.root_node().named_descendant_for_byte_range(start, end)?;
    let declarator = ancestor(name, "variable_declarator")?;
    let value = declarator.child_by_field_name("value")?;
    let declaration = parent_of(declarator)?;

    if !is_constant_declaration(&declaration, bytes) || !has_constant_type(&declaration, bytes) {
        return None;
    }
    if !is_constant_expression(&value) {
        return None;
    }
    Some(one_line(value.utf8_text(bytes).ok()?))
}

/// The nearest ancestor of `node` (itself included) of kind `kind`, stopping at the member level.
fn ancestor<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == kind {
            return Some(n);
        }
        if n.kind().ends_with("_declaration") {
            return None;
        }
        cur = parent_of(n);
    }
    None
}

/// `static final`, or a field of an interface (JLS §9.3: implicitly both).
fn is_constant_declaration(declaration: &Node, bytes: &[u8]) -> bool {
    match declaration.kind() {
        "constant_declaration" => true,
        "field_declaration" => {
            let Some(modifiers) = first_child_of_kind(declaration, "modifiers") else { return false };
            let mut cursor = modifiers.walk();
            let words: Vec<&str> = modifiers
                .children(&mut cursor)
                .filter_map(|c| c.utf8_text(bytes).ok())
                .collect();
            words.contains(&"static") && words.contains(&"final")
        }
        _ => false,
    }
}

/// A primitive, or `String` — the types a compile-time constant can have (JLS §15.29).
fn has_constant_type(declaration: &Node, bytes: &[u8]) -> bool {
    let Some(ty) = declaration.child_by_field_name("type") else { return false };
    if PRIMITIVE_TYPES.contains(&ty.kind()) {
        return true;
    }
    matches!(ty.utf8_text(bytes), Ok("String" | "java.lang.String"))
}

/// Whether `node` is built only from literals, names and operators.
fn is_constant_expression(node: &Node) -> bool {
    if !CONSTANT_PARTS.contains(&node.kind()) {
        return false;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    children.iter().all(is_constant_expression)
}

fn first_child_of_kind<'t>(node: &Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// The initializer on one line, cut when long. A wrapped `"a" +\n "b"` reads the same joined.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= MAX_CHARS {
        return joined;
    }
    let mut cut: String = joined.chars().take(MAX_CHARS).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(src: &str, field: &str) -> Option<String> {
        project_constant(src, &DeclKey::Field { owner: "p/A".to_string(), name: field.to_string() })
    }

    #[test]
    fn a_static_final_primitive_shows_its_initializer() {
        let src = "package p;\nclass A {\n  public static final int MAX = 42;\n  static final long T = 30 * 1000L;\n}\n";
        assert_eq!(constant(src, "MAX").as_deref(), Some("42"));
        assert_eq!(constant(src, "T").as_deref(), Some("30 * 1000L"));
    }

    #[test]
    fn a_string_constant_and_one_built_from_another() {
        let src = "package p;\nclass A {\n  static final String BASE = \"/api\";\n  static final String USERS = BASE + \"/users\";\n}\n";
        assert_eq!(constant(src, "BASE").as_deref(), Some("\"/api\""));
        assert_eq!(constant(src, "USERS").as_deref(), Some("BASE + \"/users\""));
    }

    #[test]
    fn an_interface_field_is_a_constant_without_the_words() {
        let src = "package p;\ninterface A {\n  int SIZE = 8;\n}\n";
        assert_eq!(constant(src, "SIZE").as_deref(), Some("8"));
    }

    #[test]
    fn what_is_not_a_constant_shows_nothing() {
        let src = "package p;\nclass A {\n  static int counter = 0;\n  final int x = 1;\n  static final Object O = new Object();\n  static final long NOW = System.currentTimeMillis();\n  static final int[] A = {1, 2};\n}\n";
        for field in ["counter", "x", "O", "NOW", "A"] {
            assert_eq!(constant(src, field), None, "{field}");
        }
    }

    #[test]
    fn a_wrapped_initializer_is_joined_on_one_line() {
        let src = "package p;\nclass A {\n  static final String Q = \"select *\"\n      + \" from t\";\n}\n";
        assert_eq!(constant(src, "Q").as_deref(), Some("\"select *\" + \" from t\""));
    }
}
