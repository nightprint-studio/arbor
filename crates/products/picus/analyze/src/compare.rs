//! Comparing rows across two files that are not written in the same dialect.
//!
//! Two INSERTs are "the same row" when they put the same values in the same
//! columns. Getting that judgement wrong in either direction is expensive, so the
//! rule here is deliberately conservative:
//!
//! * a cell that is **not a literal** (`SYSDATE`, `seq.NEXTVAL`, `a || b`) makes
//!   the whole row **incomparable**, and the caller must abstain rather than
//!   guess. Two rows whose key cells are both "computed" are not known to be
//!   equal, and claiming they are is how a tool tells someone their correct
//!   script is broken;
//! * numbers are compared **numerically**, because `1.50` and `1.5` are the same
//!   number written by two people;
//! * strings are compared **exactly**, because `'X'` and `'x'` are different data
//!   even where the identifiers around them fold.

use picus_parse::prelude::{ColumnRef, DmlShape, LiteralValue, ValueRow};

/// A row reduced to `(column, value)` pairs in a canonical order.
///
/// Sorted by column name when the statement has a column list, so two branches
/// that write the same columns in a different order still compare equal — which
/// they should, since the database does not care either.
pub type RowFingerprint = Vec<(String, String)>;

/// The fingerprint of one row, or `None` when the row cannot be compared.
pub fn row_fingerprint(shape: &DmlShape, row: &ValueRow) -> Option<RowFingerprint> {
    if row.values.is_empty() {
        return None;
    }
    let named = shape.has_column_list && shape.columns.len() == row.values.len();
    let mut out: RowFingerprint = Vec::with_capacity(row.values.len());
    for (index, cell) in row.values.iter().enumerate() {
        let value = normalise(cell.literal.as_ref()?);
        let key = if named {
            shape.columns[index].folded_name()
        } else {
            // Zero-padded so `#10` sorts after `#2` — a positional comparison
            // that reordered itself at ten columns would be a delightful bug.
            format!("#{index:04}")
        };
        out.push((key, value));
    }
    out.sort();
    Some(out)
}

/// Every comparable row of a statement, or `None` if any one of them is not.
///
/// All-or-nothing on purpose: comparing the two rows that happen to be literal
/// while ignoring the third would report a difference that is an artefact of what
/// this crate can read.
pub fn comparable_rows(shape: &DmlShape) -> Option<Vec<RowFingerprint>> {
    if shape.from_query {
        // `INSERT … SELECT` has no rows to compare; its content is whatever the
        // query returns at install time.
        return None;
    }
    shape.rows.iter().map(|row| row_fingerprint(shape, row)).collect()
}

/// The columns a statement writes, folded — comparable even when the values are
/// not.
///
/// This is the part of a cross-dialect comparison that survives `SYSDATE`: two
/// branches inserting into different column sets have diverged whatever the
/// values are, and noticing that costs nothing.
pub fn written_columns(shape: &DmlShape) -> Vec<String> {
    let mut out: Vec<String> = shape
        .columns
        .iter()
        .map(ColumnRef::folded_name)
        .chain(shape.assignments.iter().map(|a| a.column.folded_name()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A fingerprint rendered for a message: `COD='SOGLIA_SCONTO'`.
pub fn render(fingerprint: &RowFingerprint) -> String {
    fingerprint
        .iter()
        .map(|(column, value)| format!("{column}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalise(value: &LiteralValue) -> String {
    match value {
        LiteralValue::String(text) => format!("'{text}'"),
        // `1.50` and `1.5` are one number written by two people; `1e3` and `1000`
        // likewise. Anything the parser cannot read as a number keeps its text.
        LiteralValue::Number(text) => match text.parse::<f64>() {
            Ok(number) => format!("{number}"),
            Err(_) => text.clone(),
        },
        LiteralValue::Bool(flag) => flag.to_string(),
        LiteralValue::Null => "NULL".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_parse::prelude::{EngineKind, SqlParser};

    fn shapes(source: &str, engine: EngineKind) -> Vec<DmlShape> {
        SqlParser::new()
            .parse(source, engine)
            .statements
            .iter()
            .flat_map(|s| s.dml.iter().cloned())
            .collect()
    }

    fn only(source: &str, engine: EngineKind) -> DmlShape {
        shapes(source, engine).into_iter().next().expect("one DML statement")
    }

    #[test]
    fn the_same_row_written_in_two_dialects_fingerprints_the_same() {
        let oracle = only(
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
            EngineKind::Oracle,
        );
        let postgres = only(
            "insert into parametri (cod, valore) values ('SOGLIA_SCONTO', 15);",
            EngineKind::Postgres,
        );
        assert_eq!(comparable_rows(&oracle), comparable_rows(&postgres));
    }

    #[test]
    fn column_order_is_not_part_of_the_row() {
        let a = only("INSERT INTO T (A, B) VALUES (1, 2);", EngineKind::Oracle);
        let b = only("INSERT INTO T (B, A) VALUES (2, 1);", EngineKind::Oracle);
        assert_eq!(comparable_rows(&a), comparable_rows(&b));
    }

    #[test]
    fn numbers_compare_numerically_and_strings_do_not() {
        let a = only("INSERT INTO T (A) VALUES (1.50);", EngineKind::Oracle);
        let b = only("INSERT INTO T (A) VALUES (1.5);", EngineKind::Oracle);
        assert_eq!(comparable_rows(&a), comparable_rows(&b));

        let upper = only("INSERT INTO T (A) VALUES ('X');", EngineKind::Oracle);
        let lower = only("INSERT INTO T (A) VALUES ('x');", EngineKind::Oracle);
        assert_ne!(comparable_rows(&upper), comparable_rows(&lower));
    }

    #[test]
    fn a_computed_cell_makes_the_whole_row_incomparable() {
        // Two rows stamped with SYSDATE are not known to be equal, and the rules
        // must abstain rather than claim they are.
        let shape = only("INSERT INTO T (A, D) VALUES ('X', SYSDATE);", EngineKind::Oracle);
        assert_eq!(comparable_rows(&shape), None);
    }

    #[test]
    fn an_insert_from_a_query_has_nothing_to_compare() {
        let shape = only("INSERT INTO T (A) SELECT A FROM U;", EngineKind::Oracle);
        assert_eq!(comparable_rows(&shape), None);
    }

    #[test]
    fn quoting_style_does_not_change_a_value() {
        // The two dialects spell the same string three ways; the value is one.
        let plain = only("INSERT INTO T (A) VALUES ('it''s');", EngineKind::Oracle);
        let q = only("INSERT INTO T (A) VALUES (q'[it's]');", EngineKind::Oracle);
        let dollar = only("insert into t (a) values ($$it's$$);", EngineKind::Postgres);
        assert_eq!(comparable_rows(&plain), comparable_rows(&q));
        assert_eq!(comparable_rows(&plain), comparable_rows(&dollar));
    }

    #[test]
    fn the_written_columns_survive_a_value_nobody_can_compare() {
        let shape = only("INSERT INTO T (COD, DATA) VALUES ('X', SYSDATE);", EngineKind::Oracle);
        assert_eq!(written_columns(&shape), ["COD", "DATA"]);
        assert_eq!(comparable_rows(&shape), None);
    }

    #[test]
    fn a_fingerprint_reads_as_a_key_when_it_is_rendered() {
        let shape =
            only("INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');", EngineKind::Oracle);
        let rows = comparable_rows(&shape).expect("comparable");
        assert_eq!(render(&rows[0]), "COD='SOGLIA_SCONTO'");
    }
}
