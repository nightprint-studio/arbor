//! Building a [`DmlShape`] from an `insert` / `update` / `delete` / `merge` node.

use tree_sitter::Node;

use crate::dml::{Assignment, ColumnRef, DmlOperation, DmlShape, ValueCell, ValueRow};
use crate::literal;
use crate::object::ObjectKind;
use crate::walk::names::{field_ref, range_of, text_of};

/// `None` when the node is not a DML statement.
pub(crate) fn shape(node: Node, source: &str) -> Option<DmlShape> {
    let (operation, table_field) = match node.kind() {
        "insert_statement" => (DmlOperation::Insert, "table"),
        "update_statement" => (DmlOperation::Update, "table"),
        "delete_statement" => (DmlOperation::Delete, "table"),
        "merge_statement" => (DmlOperation::Merge, "target"),
        _ => return None,
    };
    let table = field_ref(node, table_field, source, ObjectKind::Table)?;

    // MERGE carries its column list inside the WHEN NOT MATCHED branch rather
    // than on the statement, so both places have to be looked at — a MERGE
    // whose INSERT has no column list is the same finding as an INSERT without
    // one, and it must not be lost to where the grammar happens to put it.
    let column_list = node
        .child_by_field_name("columns")
        .or_else(|| nested_child(node, "merge_when_clause", "column_list"));
    let columns = column_list.map(|n| columns_of(n, source)).unwrap_or_default();

    let mut rows = Vec::new();
    if let Some(values) = node.child_by_field_name("values") {
        rows = rows_of(values, source);
    }

    let mut assignments = Vec::new();
    let mut conflict = None;
    let mut where_clause = None;
    let mut returning = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "assignment" => {
                if let Some(a) = assignment(child, source) {
                    assignments.push(a);
                }
            }
            "where_clause" => where_clause = Some(range_of(child)),
            "returning_clause" => returning = Some(range_of(child)),
            "on_conflict_clause" => {
                conflict = Some(range_of(child));
                collect_nested_assignments(child, source, &mut assignments);
            }
            "merge_when_clause" => {
                // The whole WHEN block is the conflict handling; MERGE spells in
                // one statement what PostgreSQL spells with ON CONFLICT, and the
                // cross-dialect diff has to be able to line the two up.
                let range = range_of(child);
                conflict = Some(match conflict {
                    Some(existing) => crate::range::ByteRange::new(existing.start, range.end),
                    None => range,
                });
                collect_nested_assignments(child, source, &mut assignments);
                for row in nested_rows(child, source) {
                    rows.push(row);
                }
            }
            _ => {}
        }
    }

    Some(DmlShape {
        operation,
        table,
        has_column_list: column_list.is_some(),
        columns,
        rows,
        from_query: node.child_by_field_name("query").is_some(),
        assignments,
        where_clause,
        returning,
        conflict,
    })
}

fn columns_of(list: Node, source: &str) -> Vec<ColumnRef> {
    let mut out = Vec::new();
    let mut cursor = list.walk();
    for child in list.named_children(&mut cursor) {
        out.push(ColumnRef { name: text_of(child, source).to_string(), range: range_of(child) });
    }
    out
}

fn rows_of(values: Node, source: &str) -> Vec<ValueRow> {
    let mut out = Vec::new();
    let mut cursor = values.walk();
    for child in values.named_children(&mut cursor) {
        if child.kind() == "value_row" {
            out.push(value_row(child, source));
        }
    }
    out
}

/// Cells in source order, INCLUDING the bare `DEFAULT` keyword.
///
/// `DEFAULT` is an anonymous node, so taking only the named children would drop
/// it and shift every following cell one position left — silently misaligning
/// the row against the column list, which is precisely the alignment a
/// duplicate-key check depends on.
fn value_row(row: Node, source: &str) -> ValueRow {
    let mut values = Vec::new();
    let mut cursor = row.walk();
    for child in row.children(&mut cursor) {
        match child.kind() {
            "(" | ")" | "," => continue,
            _ => values.push(cell(child, source)),
        }
    }
    ValueRow { range: range_of(row), values }
}

fn cell(node: Node, source: &str) -> ValueCell {
    ValueCell {
        range: range_of(node),
        literal: literal::decode(node.kind(), text_of(node, source)),
    }
}

fn assignment(node: Node, source: &str) -> Option<Assignment> {
    let column = node.child_by_field_name("column")?;
    let value = node.child_by_field_name("value")?;
    Some(Assignment {
        column: ColumnRef { name: text_of(column, source).to_string(), range: range_of(column) },
        value: cell(value, source),
        range: range_of(node),
    })
}

fn collect_nested_assignments(node: Node, source: &str, out: &mut Vec<Assignment>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "assignment" {
            if let Some(a) = assignment(child, source) {
                out.push(a);
            }
        }
    }
}

/// The first `inner` child of the first `outer` child of `node`.
fn nested_child<'t>(node: Node<'t>, outer: &str, inner: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let outers: Vec<Node<'t>> =
        node.children(&mut cursor).filter(|c| c.kind() == outer).collect();
    for candidate in outers {
        let mut inner_cursor = candidate.walk();
        let found = candidate
            .children(&mut inner_cursor)
            .find(|c| c.kind() == inner);
        if found.is_some() {
            return found;
        }
    }
    None
}

fn nested_rows(node: Node, source: &str) -> Vec<ValueRow> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "values_clause" {
            out.extend(rows_of(child, source));
        }
    }
    out
}
