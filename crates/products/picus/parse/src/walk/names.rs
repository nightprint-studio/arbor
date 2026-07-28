//! Turning `object_name` nodes into [`ObjectRef`]s.

use tree_sitter::Node;

use crate::object::{ObjectKind, ObjectRef};
use crate::range::ByteRange;

pub(crate) fn range_of(node: Node) -> ByteRange {
    ByteRange::new(node.start_byte(), node.end_byte())
}

pub(crate) fn text_of<'a>(node: Node, source: &'a str) -> &'a str {
    range_of(node).slice(source)
}

/// The dotted components of an `object_name`, as written.
///
/// Stops at an Oracle database link (`t@dblink`): the link is not part of the
/// object's name, and letting it become the last component would make every
/// remote reference look like an object called `dblink`.
fn name_parts(node: Node, source: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "@" => break,
            "identifier" | "quoted_identifier" => parts.push(text_of(child, source).to_string()),
            _ => {}
        }
    }
    parts
}

/// Build an [`ObjectRef`] from an `object_name` node. `None` when the node has
/// no name components at all, which only happens inside an error.
pub(crate) fn object_ref(node: Node, source: &str, kind: ObjectKind) -> Option<ObjectRef> {
    let mut parts = name_parts(node, source);
    let name = parts.pop()?;
    let schema = parts.pop();
    Some(ObjectRef { kind, schema, name, range: range_of(node) })
}

/// The `object_name` in a named field, as an [`ObjectRef`].
pub(crate) fn field_ref(
    node: Node,
    field: &str,
    source: &str,
    kind: ObjectKind,
) -> Option<ObjectRef> {
    object_ref(node.child_by_field_name(field)?, source, kind)
}

/// The object type an anonymous keyword sequence names, for DROP and COMMENT ON.
///
/// The keywords are anonymous nodes, so this reads their text rather than their
/// kind — which is also why it has to be case-insensitive.
pub(crate) fn object_kind_from_keywords(words: &[String]) -> ObjectKind {
    let joined = words.join(" ").to_uppercase();
    match joined.as_str() {
        "TABLE" => ObjectKind::Table,
        "VIEW" => ObjectKind::View,
        "MATERIALIZED VIEW" => ObjectKind::MaterializedView,
        "INDEX" => ObjectKind::Index,
        "SEQUENCE" => ObjectKind::Sequence,
        "TRIGGER" => ObjectKind::Trigger,
        "FUNCTION" => ObjectKind::Function,
        "PROCEDURE" => ObjectKind::Procedure,
        "PACKAGE" => ObjectKind::Package,
        "PACKAGE BODY" => ObjectKind::PackageBody,
        "TYPE" => ObjectKind::Type,
        "SCHEMA" => ObjectKind::Schema,
        "SYNONYM" => ObjectKind::Synonym,
        "COLUMN" => ObjectKind::Column,
        "CONSTRAINT" => ObjectKind::Constraint,
        "ROLE" => ObjectKind::Role,
        "DATABASE" => ObjectKind::Database,
        "TABLESPACE" => ObjectKind::Tablespace,
        "DOMAIN" => ObjectKind::Domain,
        "EXTENSION" => ObjectKind::Extension,
        _ => ObjectKind::Unknown,
    }
}

/// The anonymous keyword words that precede the first `object_name` of a node —
/// the object type of a `DROP …` or `COMMENT ON …`.
pub(crate) fn leading_keywords(node: Node, source: &str, skip_first: usize) -> Vec<String> {
    let mut words = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).skip(skip_first) {
        if child.is_named() {
            break;
        }
        let text = text_of(child, source);
        if text == "(" || text == "," {
            break;
        }
        words.push(text.to_string());
    }
    words
}
