//! Query execution wire types — the shapes behind the scrolling-results contract.
//!
//! ## Why a result has an identity
//!
//! A grid that scrolls has to ask for row 12 000 of something. "Something" cannot
//! be the SQL text: re-running a statement to serve a later window is only correct
//! when the statement has a total order, and almost none do — without an explicit
//! `ORDER BY` a server is free to return the same rows in a different sequence, so
//! paging by `OFFSET` can repeat a row in one window and skip it in the next while
//! the user simply scrolls. So a read **opens a result** ([`ExecuteResult::result_id`])
//! that the engine holds open over one fixed snapshot, and every window
//! ([`ResultWindow`]) is answered from that.
//!
//! A held result is a resource on somebody's database. It is closed explicitly
//! ([`DbSession::close_result`](crate::provider::DbSession::close_result)), and the
//! engine is required to bound the ones nobody closes — see the trait's contract.
//!
//! ## Why the row total arrives twice
//!
//! The scrollbar needs a length immediately; an exact count of a large result can
//! take seconds. So [`ExecuteResult::estimated_rows`] carries the planner's guess at
//! once (rendered with a `~`), and the exact figure is asked for separately
//! ([`ResultCount`]) and replaces it when it lands. `total_rows` exists on the first
//! reply for the engine that already knows the exact number for free; it is `None`
//! whenever knowing would have cost a scan.

use serde::{Deserialize, Serialize};

use crate::schema::Column;

/// How many rows a freshly opened result hands back without being asked.
///
/// The first window is a backend decision rather than a parameter, because
/// `picus_execute` has no business taking a page size: the caller does not know
/// what it is about to get, and a grid that has just been handed a result has not
/// yet been scrolled. Later windows are explicit — by then the viewport is known.
pub const DEFAULT_WINDOW_ROWS: u32 = 500;

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

/// The outcome of running one statement — **every** statement.
///
/// One shape for reads and writes alike, so the caller never has to classify SQL
/// before sending it. Which half is filled in says what happened:
///
/// * a **read** carries `result_id`, `columns` and the first window of `rows`;
/// * a **write or session statement** carries `affected` and leaves `result_id`
///   and `columns` `None` — nothing was opened, so there is nothing to close.
///
/// The `Option` fields serialise as explicit `null`s rather than being skipped:
/// the frontend reads one stable object, and "absent" and "null" are the same
/// answer to "is there a result to scroll?".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    /// Names the held result for [`ResultWindow`] / [`ResultCount`] and for the
    /// close. `None` when the statement opened nothing.
    pub result_id: Option<String>,
    /// The columns, known even when the result is empty. `None` when the statement
    /// produced no result set at all.
    pub columns: Option<Vec<Column>>,
    /// The first window. Never the whole result.
    pub rows: Vec<Vec<CellValue>>,
    /// The planner's guess at the total, for the scrollbar — shown with a `~`.
    /// `None` when the engine could not produce one, which must render as "unknown"
    /// and never as zero.
    pub estimated_rows: Option<i64>,
    /// The exact total, when it was free. Normally `None`: the exact figure is
    /// asked for separately so counting never delays the first rows.
    pub total_rows: Option<i64>,
    /// Server-side elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Rows in `rows` — the first window's size, not the result's.
    pub row_count: usize,
    /// True when this first window already reached the end, so a grid with fewer
    /// rows than a window knows not to ask for more.
    pub end_of_result: bool,
    /// Rows a write reported changing. `None` for a statement that returned a
    /// result set.
    pub affected: Option<u64>,
}

/// One window over a held result.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultWindow {
    /// Echoed back from the request. Load-bearing: windows are asked for while
    /// scrolling and the replies do not necessarily arrive in order, so the caller
    /// has to be able to tell which question this answers before it paints.
    pub offset: u64,
    pub rows: Vec<Vec<CellValue>>,
    /// True when this window ran to the end of the result.
    pub end_of_result: bool,
}

/// The exact number of rows in a held result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultCount {
    pub total: i64,
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

    #[test]
    fn a_write_says_null_rather_than_omitting_the_result() {
        // The frontend switches on `resultId`; a missing key and an explicit null
        // must not be two different answers to the same question.
        let json = serde_json::to_value(ExecuteResult {
            affected: Some(3),
            end_of_result: true,
            ..Default::default()
        })
        .unwrap();

        assert_eq!(json["resultId"], serde_json::Value::Null);
        assert_eq!(json["columns"], serde_json::Value::Null);
        assert_eq!(json["affected"], 3);
        assert!(json.as_object().unwrap().contains_key("estimatedRows"));
        assert!(json.as_object().unwrap().contains_key("totalRows"));
    }

    #[test]
    fn the_wire_names_are_the_ones_the_grid_reads() {
        let json = serde_json::to_value(ExecuteResult::default()).unwrap();
        for key in
            ["resultId", "columns", "rows", "estimatedRows", "totalRows", "elapsedMs", "rowCount", "endOfResult", "affected"]
        {
            assert!(json.get(key).is_some(), "missing `{key}`");
        }

        let window = serde_json::to_value(ResultWindow::default()).unwrap();
        for key in ["offset", "rows", "endOfResult"] {
            assert!(window.get(key).is_some(), "missing `{key}`");
        }

        assert_eq!(serde_json::to_value(ResultCount { total: 7 }).unwrap()["total"], 7);
    }
}
