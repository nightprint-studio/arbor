//! [`DmlShape`] — enough structure for the checks `picus-analyze` owes.
//!
//! Three of them shaped this type:
//!
//! * *duplicate key* — needs the decoded literal of each value cell, per row;
//! * *column-less INSERT* — needs to know that the column list was **absent**,
//!   which is why [`DmlShape::columns`] being empty is not the same fact as
//!   [`DmlShape::has_column_list`] being false;
//! * *unguarded UPDATE / DELETE* — needs the presence, and the range, of WHERE.
//!
//! Everything is a range as well as a value, so a rewriter can replace a single
//! cell without reprinting the statement.

use serde::{Deserialize, Serialize};

use crate::literal::LiteralValue;
use crate::object::ObjectRef;
use crate::range::ByteRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DmlOperation {
    Insert,
    Update,
    Delete,
    Merge,
}

/// A column as written in an INSERT's column list or an UPDATE's SET.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnRef {
    pub name: String,
    pub range: ByteRange,
}

impl ColumnRef {
    /// Comparison form; same folding rule as [`ObjectRef::folded_name`].
    ///
    /// [`ObjectRef::folded_name`]: crate::object::ObjectRef::folded_name
    pub fn folded_name(&self) -> String {
        if self.name.len() >= 2 && self.name.starts_with('"') && self.name.ends_with('"') {
            self.name[1..self.name.len() - 1].replace("\"\"", "\"")
        } else {
            self.name.to_uppercase()
        }
    }
}

/// One value in a VALUES row or on the right of an assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueCell {
    pub range: ByteRange,
    /// The decoded value when the cell is a literal. `None` for anything
    /// computed — `SYSDATE`, `seq.NEXTVAL`, `a || b` — which is exactly the
    /// distinction a duplicate-key check needs: two rows whose key cells are
    /// both `None` are not known to be duplicates.
    pub literal: Option<LiteralValue>,
}

/// One `( … )` of a VALUES clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRow {
    pub range: ByteRange,
    pub values: Vec<ValueCell>,
}

/// One `col = value` of an UPDATE's SET or a MERGE's UPDATE branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub column: ColumnRef,
    pub value: ValueCell,
    pub range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DmlShape {
    pub operation: DmlOperation,
    /// The table the statement writes to.
    pub table: ObjectRef,
    /// The explicit column list, in source order. Empty when there was none.
    pub columns: Vec<ColumnRef>,
    /// Whether a column list was written at all. `INSERT INTO t VALUES (…)` is
    /// a finding on its own — it silently binds to the table's current column
    /// order, so adding a column upstream breaks the script — and it is only
    /// distinguishable from `INSERT INTO t (a) …` by this flag.
    pub has_column_list: bool,
    /// The VALUES rows. Empty for `INSERT … SELECT`.
    pub rows: Vec<ValueRow>,
    /// True when the source of rows is a query rather than a VALUES list.
    pub from_query: bool,
    /// UPDATE's SET, or the UPDATE branch of a MERGE.
    pub assignments: Vec<Assignment>,
    /// The WHERE clause, when there is one. Absence is the finding.
    pub where_clause: Option<ByteRange>,
    /// `RETURNING …`, when present.
    pub returning: Option<ByteRange>,
    /// The conflict-handling clause: PostgreSQL's `ON CONFLICT …` or the
    /// `WHEN [NOT] MATCHED` block of a MERGE. Same slot for both, because they
    /// are the same intention written two ways.
    pub conflict: Option<ByteRange>,
}

impl DmlShape {
    /// The cells of `row` that correspond to the named columns, in the order the
    /// names were given. `None` when the statement has no column list, or when a
    /// name is not in it, or when the row is shorter than the column list — all
    /// three being cases where a positional match would be a guess.
    pub fn key_cells<'a>(&'a self, row: &'a ValueRow, key: &[String]) -> Option<Vec<&'a ValueCell>> {
        if !self.has_column_list {
            return None;
        }
        let folded: Vec<String> = self.columns.iter().map(ColumnRef::folded_name).collect();
        key.iter()
            .map(|wanted| {
                let want = wanted.to_uppercase();
                let idx = folded.iter().position(|c| *c == want)?;
                row.values.get(idx)
            })
            .collect()
    }
}
