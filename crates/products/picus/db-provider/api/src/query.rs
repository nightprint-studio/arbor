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

/// One value bound to a placeholder.
///
/// Bound, never interpolated. That is the entire point: a value spliced into the
/// SQL text has to be quoted by whoever splices it, and "whoever splices it" is
/// how every injection and every mangled apostrophe in a description field has
/// ever happened. The driver sends these beside the statement, and the server —
/// which knows the column's type — does the conversion.
///
/// The variants are deliberately few. A bind is what the user typed into a box;
/// carrying a date type here would mean parsing their text into one, in a product
/// that has two engines with different date syntaxes. `Text` and the server's own
/// coercion is the honest answer, and `Null` is a real NULL rather than an empty
/// string for the same reason [`CellValue::Null`] is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindValue {
    /// Must stay first: untagged deserialisation tries the variants in order.
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

/// A placeholder found in a statement, in the order the engine will bind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindSlot {
    /// What the user sees: `:CODICE` on Oracle, `$1` on PostgreSQL.
    pub label: String,
    /// 1-based position in the bind list.
    pub index: u32,
}

/// Whether a read may replace large-object columns with their size.
///
/// The engine masks large objects by returning their byte length in place of the
/// value, fetching the real bytes only when a cell is opened — which needs the row
/// to be *addressable* (see [`ExecuteResult::row_key`]). The caller resolves that and
/// tells the engine which way to go: masking a cell nobody can ever open would show a
/// size that is a dead end, so a caller that finds no key asks for the value instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LobMasking {
    /// Mask large objects — the rows are addressable, so a masked cell can be read
    /// on demand later.
    #[default]
    Auto,
    /// Do not mask — there is no key to fetch a masked cell by, so the value is read
    /// whole rather than shown as a size that could never be expanded.
    Off,
}

/// Where one result column is read from.
///
/// ## Sparse, and it carries its own position
///
/// A column with no origin — a computed expression, a literal, an engine that does
/// not report one — simply has no entry, and the entry that does exist names the
/// index it describes. So a caller that hides the trailing columns
/// ([`ExecuteResult::hidden_columns`]) *filters* this list instead of slicing it in
/// step with `columns`, and there is no length to keep aligned: the alignment bug
/// this shape is avoiding is the kind that shows the wrong table's name against a
/// column and is never noticed, because a plausible name in the wrong place looks
/// exactly like a right one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSource {
    /// Position in [`ExecuteResult::columns`] this describes.
    pub index: usize,
    /// The relation the column is read from, as the catalogue spells it.
    pub table: String,
    /// The column's name **in that relation**. Differs from the result column's own
    /// name when the projection aliased it, and is empty when the engine named a
    /// relation but not a column within it — a row address such as `ctid`.
    pub name: String,
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
    /// Columns whose value was **not fetched** — a large object, replaced in the
    /// projection by its size in bytes.
    ///
    /// A grid showing one of these is showing a number where a value belongs, and it
    /// has to say so: the cell renders as a placeholder and the real value is read
    /// on demand. Empty for every statement the caller wrote itself, because Picus
    /// only rewrites a projection it composed.
    #[serde(default)]
    pub masked_columns: Vec<String>,
    /// Columns present in every row but **hidden from the grid** — the row key Picus
    /// injected so a masked cell can be addressed when the query did not select it.
    ///
    /// They sit at the end of `columns`/`rows`, so hiding them is a matter of not
    /// rendering the trailing cells; everything that reads a row by index still sees
    /// them. Empty unless a key was injected — the engine never fills this, the
    /// caller that did the injection does.
    #[serde(default)]
    pub hidden_columns: Vec<String>,
    /// The columns that identify one row, for reading a masked large object back
    /// (`WHERE key = …`). Either the table's primary key (visible or injected) or an
    /// engine row address such as `ctid` (always injected, always hidden). Empty when
    /// the rows are not addressable — in which case nothing was masked either.
    #[serde(default)]
    pub row_key: Vec<String>,
    /// Which relation each column is read from, for those that are read from one.
    ///
    /// Sparse — see [`ColumnSource`]. Empty in three different situations that a
    /// caller must treat identically: the engine does not report origins at all, the
    /// statement could not be described, or nothing in the result came from a
    /// relation. All three mean *no claim is being made*, which is not the same as
    /// "these columns have no source" and must never be rendered as if it were.
    #[serde(default)]
    pub column_sources: Vec<ColumnSource>,
    /// The statement that actually ran against the data, when it differs from the one
    /// the caller sent — because a key was injected into its projection, or its large
    /// objects were wrapped into sizes. `None` when what ran is what was asked for.
    ///
    /// This is the *logical* effective statement (the rewritten/wrapped SELECT), not
    /// the cursor plumbing around it — it exists so the history can show "you asked X,
    /// Y ran".
    #[serde(default)]
    pub effective_sql: Option<String>,
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
        for key in [
            "resultId", "columns", "rows", "estimatedRows", "totalRows", "elapsedMs", "rowCount",
            "endOfResult", "affected", "maskedColumns", "hiddenColumns", "rowKey", "columnSources",
            "effectiveSql",
        ] {
            assert!(json.get(key).is_some(), "missing `{key}`");
        }

        let window = serde_json::to_value(ResultWindow::default()).unwrap();
        for key in ["offset", "rows", "endOfResult"] {
            assert!(window.get(key).is_some(), "missing `{key}`");
        }

        assert_eq!(serde_json::to_value(ResultCount { total: 7 }).unwrap()["total"], 7);
    }

    #[test]
    fn a_source_survives_hiding_the_trailing_columns() {
        // The shape's whole reason for existing: an injected key column is dropped
        // from the end of `columns`, and the sources of what remains must come
        // through unshifted and un-truncated by their own arithmetic.
        let sources = vec![
            ColumnSource { index: 0, table: "comunicazioni".into(), name: "id".into() },
            ColumnSource { index: 2, table: "enti".into(), name: "denominazione".into() },
            ColumnSource { index: 3, table: "comunicazioni".into(), name: "ctid".into() },
        ];
        let visible = 3; // the fourth column is the injected key

        let kept: Vec<_> = sources.iter().filter(|s| s.index < visible).collect();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].index, 2, "a filter must not renumber what it keeps");
        assert_eq!(kept[1].table, "enti");
    }

    #[test]
    fn a_source_is_camel_case_on_the_wire() {
        let json = serde_json::to_value(ColumnSource {
            index: 4,
            table: "comunicazioni".into(),
            name: "data_invio".into(),
        })
        .unwrap();
        assert_eq!(json["index"], 4);
        assert_eq!(json["table"], "comunicazioni");
        assert_eq!(json["name"], "data_invio");
    }
}
