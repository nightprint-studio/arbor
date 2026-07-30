//! Turning the simple protocol's reply into columns and cells.
//!
//! Execution goes through the **simple query protocol**, which hands every value
//! back as text exactly as the server would print it. That is deliberate, and it is
//! the right trade for a maintenance tool:
//!
//! * a `timestamptz`, a `numeric(38,10)` and a domain type come back looking the way
//!   they will look in the script the user is about to write — no client-side
//!   reformatting silently changing what they see;
//! * an unknown or exotic type can never fail a whole result set, because nothing is
//!   being decoded into a Rust type;
//! * `NULL` stays distinguishable from the empty string, which in a tool that writes
//!   UPDATE statements is not a detail.
//!
//! The cost is that the simple protocol carries no type information. So it is asked
//! for separately with a `prepare` (best-effort: `SET` and multi-statement input are
//! not preparable, and then the columns are simply untyped) and used for one thing
//! only — deciding whether a column is numeric, so the grid can right-align it and
//! the value survives as a number rather than a string. Text columns are never
//! parsed as numbers: an account code of `007` must not become `7`.

use picus_db_api::prelude::{CellValue, Column};
use tokio_postgres::types::Type;
use tokio_postgres::SimpleQueryMessage;

/// What one simple-query round trip produced.
pub struct Fetched {
    /// The columns of the first result set. Known even when it returned no rows,
    /// because the server describes the row shape before sending any — which is
    /// what lets an empty grid still draw its headers.
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<CellValue>>,
    /// True when the statement produced a result set at all — the difference
    /// between "a SELECT that matched nothing" and "an UPDATE".
    pub had_result_set: bool,
    /// The row count reported by the **last** completed statement. For a write that
    /// is what it changed; for `MOVE FORWARD ALL` it is how far it moved.
    pub last_command_count: Option<u64>,
}

/// Shape a reply.
///
/// `types` (from a best-effort `prepare`) decides only which columns are numeric;
/// the values themselves are always the server's own text.
///
/// `limit` is a memory backstop, not the mechanism — the statement has normally
/// already been bounded by the server (`FETCH FORWARD n`). It still matters for the
/// statements that cannot be bounded that way, where it is all that keeps an
/// unbounded result from being held in this process.
///
/// Only the **first** result set is collected. A multi-statement paste can produce
/// several with different shapes, and a grid can draw one: showing the first and
/// reporting the last statement's count is legible, where interleaving rows of
/// different widths is not.
pub fn collect(
    messages: Vec<SimpleQueryMessage>,
    types: Option<&[(String, Type)]>,
    limit: u32,
) -> Fetched {
    let mut out = Fetched {
        columns: Vec::new(),
        rows: Vec::new(),
        had_result_set: false,
        last_command_count: None,
    };
    let mut numeric: Vec<bool> = Vec::new();
    let mut collecting = false;

    for message in messages {
        match message {
            SimpleQueryMessage::RowDescription(described) => {
                if out.had_result_set {
                    // A second result set: stop, keep the first.
                    collecting = false;
                    continue;
                }
                out.had_result_set = true;
                collecting = true;
                out.columns = described
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Column {
                        name: c.name().to_string(),
                        data_type: column_type(types, i).map(|t| t.name().to_string()).unwrap_or_default(),
                        primary_key: false,
                        not_null: false,
                        default_value: None,
                    })
                    .collect();
                numeric = (0..out.columns.len())
                    .map(|i| column_type(types, i).is_some_and(is_numeric))
                    .collect();
            }
            SimpleQueryMessage::Row(row) => {
                if !collecting || out.rows.len() as u32 >= limit {
                    continue;
                }
                out.rows.push(
                    (0..row.len())
                        .map(|i| cell(row.get(i), numeric.get(i).copied().unwrap_or(false)))
                        .collect(),
                );
            }
            SimpleQueryMessage::CommandComplete(n) => out.last_command_count = Some(n),
            _ => {}
        }
    }

    out
}

fn column_type(types: Option<&[(String, Type)]>, index: usize) -> Option<&Type> {
    types?.get(index).map(|(_, ty)| ty)
}

/// Turn one text value into a cell.
///
/// Numeric columns become numbers so the grid can right-align them with tabular
/// figures; everything else stays the server's text. A `numeric` too wide for an
/// `f64` deliberately stays text rather than being silently rounded — losing
/// precision in a tool that writes SQL is worse than losing the alignment.
pub(crate) fn cell(value: Option<&str>, numeric: bool) -> CellValue {
    let Some(text) = value else { return CellValue::Null };
    if !numeric {
        return CellValue::Text(text.to_string());
    }
    if let Ok(i) = text.parse::<i64>() {
        return CellValue::Int(i);
    }
    match text.parse::<f64>() {
        // Round-trip check: only take the float when it prints back identically,
        // so a high-precision decimal keeps every digit as text.
        Ok(f) if format!("{f}") == text => CellValue::Float(f),
        _ => CellValue::Text(text.to_string()),
    }
}

/// Is this a type the grid should treat as a number?
///
/// Shared with [`crate::bind`], which reads a bound statement through a text cast
/// and still has to decide which columns the grid right-aligns — the same question,
/// and it must not have two answers.
pub(crate) fn is_numeric(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::INT2 | Type::INT4 | Type::INT8 | Type::FLOAT4 | Type::FLOAT8 | Type::NUMERIC | Type::OID
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_and_empty_string_stay_different() {
        assert_eq!(cell(None, false), CellValue::Null);
        assert_eq!(cell(Some(""), false), CellValue::Text(String::new()));
    }

    #[test]
    fn text_columns_are_never_parsed_as_numbers() {
        // An account code must survive with its leading zeros.
        assert_eq!(cell(Some("007"), false), CellValue::Text("007".to_string()));
    }

    #[test]
    fn numeric_columns_become_numbers() {
        assert_eq!(cell(Some("42"), true), CellValue::Int(42));
        assert_eq!(cell(Some("-1"), true), CellValue::Int(-1));
        assert_eq!(cell(Some("1.5"), true), CellValue::Float(1.5));
    }

    #[test]
    fn a_decimal_too_precise_for_f64_stays_text() {
        let wide = "0.12345678901234567890123456789";
        assert_eq!(
            cell(Some(wide), true),
            CellValue::Text(wide.to_string()),
            "precision matters more than alignment in a tool that writes SQL"
        );
    }

    #[test]
    fn an_empty_reply_says_nothing_happened() {
        let out = collect(Vec::new(), None, 10);
        assert!(!out.had_result_set);
        assert!(out.columns.is_empty());
        assert_eq!(out.last_command_count, None);
    }
}
