//! Reading a [`SelectShape`] off a `select_statement` node.
//!
//! Small on purpose (see [`crate::select`]): it looks only at the top-level
//! projection and the clauses that would make injecting a column unsafe. It never
//! descends into a subquery — a key is injected into the *outer* SELECT, and what an
//! inner one projects is its own business.

use tree_sitter::Node;

use crate::object::{fold, ObjectKind};
use crate::select::SelectShape;

use super::names::{object_ref, text_of};

/// The shape of a top-level SELECT, or `None` when this node is not one.
pub(crate) fn shape_of(body: Node, source: &str) -> Option<SelectShape> {
    if body.kind() != "select_statement" {
        return None;
    }

    // The core that carries the projection. A `select_statement` whose top child is a
    // `set_operation` (UNION / INTERSECT / …), a bare `values_clause`, or a `subquery`
    // has no single projection to splice into — that alone makes it un-injectable.
    let Some(core) = first_child_of_kind(body, "select_core") else {
        return Some(SelectShape { not_injectable: true, ..Default::default() });
    };
    let Some(list) = core.child_by_field_name("list") else {
        return Some(SelectShape { not_injectable: true, ..Default::default() });
    };

    let mut shape = SelectShape { select_list_end: list.end_byte(), ..Default::default() };

    // Clauses on the core that make the rows something other than "one per source
    // row", so an added column would either change the answer or be rejected.
    let mut cursor = core.walk();
    for child in core.children(&mut cursor) {
        match child.kind() {
            "DISTINCT" | "group_by_clause" | "grouping_sets_clause" | "having_clause" => {
                shape.not_injectable = true;
            }
            _ => {}
        }
    }

    let mut has_complex = false;
    let mut items = list.walk();
    for item in list.children(&mut items) {
        if item.kind() != "select_item" {
            continue;
        }
        match item_output(item, source) {
            ItemOutput::Star => shape.star = true,
            ItemOutput::Name(name) => shape.outputs.push(name),
            // A computed item (a function, a CASE, an arithmetic expression …). Adding
            // a key beside an aggregate with no GROUP BY would be rejected by the
            // server, and there is no cheap, dialect-independent way to tell an
            // aggregate from a scalar here — so any expression stands the injection
            // down. It only costs masking on that one query.
            ItemOutput::Complex => has_complex = true,
        }
    }
    shape.not_injectable = shape.not_injectable || has_complex;

    Some(shape)
}

enum ItemOutput {
    Star,
    Name(String),
    Complex,
}

/// The output name of one `select_item`, folded — or that it is a star / a computed
/// expression.
fn item_output(item: Node, source: &str) -> ItemOutput {
    let alias = item.child_by_field_name("alias");

    // The projected expression is the first named child that is not the alias.
    let mut cursor = item.walk();
    let expr = item
        .named_children(&mut cursor)
        .find(|ch| alias.map(|a| a.id()) != Some(ch.id()));

    if let Some(e) = expr {
        if e.kind() == "all_columns" {
            return ItemOutput::Star;
        }
    }

    // An alias names the output whatever the expression was.
    if let Some(a) = alias {
        return ItemOutput::Name(fold(text_of(a, source)));
    }

    match expr {
        // A plain column reference: its output name is its trailing identifier.
        Some(e) if e.kind() == "object_name" => object_ref(e, source, ObjectKind::Column)
            .map(|r| ItemOutput::Name(r.folded_name()))
            .unwrap_or(ItemOutput::Complex),
        _ => ItemOutput::Complex,
    }
}

/// The first direct child of `node` of the given kind.
fn first_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;
    use picus_types::prelude::{DialectScope, EngineKind};

    fn shape(sql: &str) -> crate::select::SelectShape {
        let parsed = parse(sql, DialectScope::One(EngineKind::Postgres));
        parsed
            .statements
            .first()
            .and_then(|s| s.select.clone())
            .unwrap_or_else(|| panic!("no select shape for {sql:?}"))
    }

    #[test]
    fn a_plain_projection_lists_its_columns_and_marks_the_splice_point() {
        let s = shape("SELECT nome, allegato FROM documenti");
        assert!(!s.star);
        assert!(!s.not_injectable);
        assert_eq!(s.outputs, vec!["NOME".to_string(), "ALLEGATO".to_string()]);
        // The splice point is just past `allegato`, before the space and `FROM`.
        assert_eq!(&"SELECT nome, allegato FROM documenti"[..s.select_list_end], "SELECT nome, allegato");
    }

    #[test]
    fn a_star_is_a_star() {
        assert!(shape("SELECT * FROM t").star);
        assert!(shape("SELECT t.* FROM t").star);
    }

    #[test]
    fn a_qualified_column_folds_to_its_trailing_name() {
        let s = shape("SELECT d.nome, d.allegato FROM documenti d");
        assert_eq!(s.outputs, vec!["NOME".to_string(), "ALLEGATO".to_string()]);
    }

    #[test]
    fn an_alias_is_the_output_name() {
        let s = shape("SELECT nome AS n, allegato FROM documenti");
        assert_eq!(s.outputs, vec!["N".to_string(), "ALLEGATO".to_string()]);
    }

    #[test]
    fn the_shapes_a_key_must_not_be_injected_into() {
        for sql in [
            "SELECT DISTINCT nome FROM t",
            "SELECT count(*) FROM t",
            "SELECT nome, count(*) FROM t GROUP BY nome",
            "SELECT nome FROM t HAVING count(*) > 1",
            "SELECT nome FROM a UNION SELECT nome FROM b",
            "SELECT upper(nome) FROM t",
        ] {
            assert!(shape(sql).not_injectable, "{sql} should not be injectable");
        }
    }

    #[test]
    fn a_plain_column_beside_an_expression_is_still_not_injectable() {
        // The expression could be an aggregate; we cannot tell cheaply, so we refuse.
        assert!(shape("SELECT nome, allegato, upper(nome) FROM documenti").not_injectable);
    }
}

