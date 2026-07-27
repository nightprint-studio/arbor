//! Query execution wire types.
//!
//! Like [`crate::schema`], serialised to the exact shapes the frontend's
//! `QueryResult` already renders.

use serde::{Deserialize, Serialize};

use crate::schema::Column;

/// One cell.
///
/// `Null` is a real SQL NULL and is **not** the empty string: the grid renders
/// them differently on purpose, because in a maintenance tool confusing the two is
/// how a bad UPDATE gets written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellValue {
    /// Must stay the first variant: untagged deserialisation tries them in order,
    /// and `null` has to land here.
    Null,
    Int(i64),
    Float(f64),
    /// Everything the driver cannot hand over as a number — including dates,
    /// which are formatted server-side style and shown verbatim.
    Text(String),
}

impl CellValue {
    /// Build from an optional string, mapping `None` to a real NULL.
    pub fn text(v: Option<String>) -> Self {
        match v {
            Some(s) => Self::Text(s),
            None => Self::Null,
        }
    }
}

/// The outcome of a statement that returned rows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<CellValue>>,
    /// Server-side elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Rows actually fetched — may be capped by the row limit.
    pub row_count: usize,
    /// True when the row limit cut the result short, so the UI can say so rather
    /// than let the user believe they saw everything.
    pub truncated: bool,
    /// For a statement that returned no rows (`INSERT`, `UPDATE`, DDL): what the
    /// server said it did (`UPDATE 3`). Empty for a SELECT.
    #[serde(default)]
    pub command_tag: String,
}

/// A page of a relation's rows — the table tab's Data view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowPage {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<CellValue>>,
    /// Zero-based index of the first row in this page.
    pub offset: u64,
    /// Total rows in the relation when known cheaply — `None` rather than a
    /// count(*) that would scan a hundred-million-row table to draw a page number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_json_null_and_survives_a_round_trip() {
        let row = vec![
            CellValue::Null,
            CellValue::Int(42),
            CellValue::Text(String::new()),
            CellValue::Float(1.5),
        ];
        let json = serde_json::to_string(&row).unwrap();
        assert_eq!(json, r#"[null,42,"",1.5]"#, "an empty string must not become null");

        let back: Vec<CellValue> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
    }
}
