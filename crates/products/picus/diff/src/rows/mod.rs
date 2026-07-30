//! Comparing two sets of rows.
//!
//! Used for three things that are the same problem: the contents of a table in
//! two databases, the contents of a table against what the install scripts put
//! there, and the results of two queries side by side. None of them involves a
//! connection, which is why they can all be one function.
//!
//! ## Matching
//!
//! Rows are matched **by key** when there is one. The key is a parameter and not
//! something this crate discovers: for a table the caller reads it from the
//! catalogue, for a query the user names it, and for a comparison against a
//! script repository it is whatever the `INSERT` statements identify a row by.
//! [`RowCompareOptions::resolve`] applies the precedence — the rule the user
//! wrote wins over the catalogue — in one place, so every call site agrees.
//!
//! Without a key the comparison is **positional**, which is only meaningful if
//! both sides were read in the same order. This crate cannot enforce that (it
//! does not do the reading), so [`TableRules::order_by`] exists to carry the
//! ordering the caller has to apply, and the result says
//! [`RowsComparison::keyed`] so nobody reads a positional answer as a keyed one.
//!
//! ## Honesty about what was left out
//!
//! Differences are **counted in full** and *listed* up to the configured cap.
//! `only_in_a.len()` is what you can show; `only_in_a_total` is what there is.
//! A truncated list without the total would let a report say "3 differences" for
//! a table with four thousand.
//!
//! [`TableRules::order_by`]: crate::config::TableRules::order_by

mod pairing;

use serde::{Deserialize, Serialize};

use crate::config::{ColumnFilter, ContentCheck};
use crate::error::{DiffError, Side};
use crate::names::{fold_name, matches_any, missing_from};
use crate::rows::pairing::{compare_by_key, compare_positionally};
use crate::value::DiffValue;

/// Rows as they were read: a header and the values under it.
///
/// Both sides are `RowSet`s whatever they came from — a table, a query, or a
/// script's `INSERT` statements rendered into cells.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<DiffValue>>,
}

impl RowSet {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<DiffValue>>) -> Self {
        Self { columns, rows }
    }

    /// Position of a column. When a result names the same column twice — legal in
    /// a hand-written `SELECT` — the first one wins, because that is the one a
    /// reader assumes they are talking about.
    pub fn column_index(&self, name: &str, case_insensitive: bool) -> Option<usize> {
        let wanted = fold_name(name, case_insensitive);
        self.columns.iter().position(|c| fold_name(c, case_insensitive) == wanted)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// How to compare two [`RowSet`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowCompareOptions {
    /// Columns that identify a row. Empty means positional.
    pub key: Vec<String>,
    /// Columns to compare. Empty means every column both sides have.
    pub columns: Vec<String>,
    /// Globs excluded from the comparison (but still shown on rows that exist on
    /// one side only, since those rows are not a per-column judgement).
    pub ignore_columns: Vec<String>,
    /// Cap on **listed** differences. `None` lists all of them.
    pub max_differences: Option<usize>,
    pub case_insensitive_names: bool,
}

impl Default for RowCompareOptions {
    fn default() -> Self {
        Self {
            key: Vec::new(),
            columns: Vec::new(),
            ignore_columns: Vec::new(),
            max_differences: Some(50),
            case_insensitive_names: true,
        }
    }
}

impl RowCompareOptions {
    /// The options for one relation, resolving the two sources of a key.
    ///
    /// `catalog_key` is what the caller read from the database; the per-relation
    /// rule overrides it. That order is the point: the catalogue is right about
    /// tables and silent about queries and about the table whose "real" identity
    /// is a business code rather than the surrogate key somebody bolted on.
    pub fn resolve(
        content: &ContentCheck,
        columns: &ColumnFilter,
        table: &str,
        catalog_key: &[String],
        case_insensitive: bool,
    ) -> Self {
        let rules = content.rules_for(table, case_insensitive);
        let mut ignore = columns.ignore_patterns.clone();
        if let Some(r) = rules {
            ignore.extend(r.ignore_columns.iter().cloned());
        }
        Self {
            key: match rules {
                Some(r) if !r.primary_key.is_empty() => r.primary_key.clone(),
                _ => catalog_key.to_vec(),
            },
            columns: rules.map(|r| r.columns.clone()).unwrap_or_default(),
            ignore_columns: ignore,
            max_differences: match content.max_differences_shown {
                0 => None,
                n => Some(n),
            },
            case_insensitive_names: case_insensitive,
        }
    }
}

/// Which row a difference is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RowKey {
    /// Positional comparison: the 0-based index in both reads.
    Position(usize),
    /// Keyed comparison: the key columns' values, in [`RowsComparison::key`]
    /// order.
    Values(Vec<DiffValue>),
}

/// A row that exists on one side only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub key: RowKey,
    /// Values in [`RowsComparison::compared_columns`] order.
    pub values: Vec<DiffValue>,
}

/// One cell that differs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDiff {
    pub column: String,
    pub a: DiffValue,
    pub b: DiffValue,
}

/// A row present on both sides with a different value somewhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedRow {
    pub key: RowKey,
    /// Only the cells that differ — never the whole row.
    pub cells: Vec<CellDiff>,
}

/// The result of comparing two [`RowSet`]s.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsComparison {
    /// What was compared: a relation name, or whatever the caller calls the pair
    /// of queries.
    pub label: String,
    /// The key columns actually used. Empty when the match was positional.
    pub key: Vec<String>,
    pub keyed: bool,
    pub compared_columns: Vec<String>,
    /// Columns one side has and the other does not. Not a per-row difference, but
    /// it changes what "the rows are identical" is worth, so it is reported.
    pub columns_only_in_a: Vec<String>,
    pub columns_only_in_b: Vec<String>,
    pub rows_a: usize,
    pub rows_b: usize,
    /// Rows paired up with nothing different in them.
    pub matched: usize,
    pub only_in_a: Vec<DiffRow>,
    pub only_in_b: Vec<DiffRow>,
    pub changed: Vec<ChangedRow>,
    pub only_in_a_total: usize,
    pub only_in_b_total: usize,
    pub changed_total: usize,
    /// Keys that identify more than one row. The key is then not a key, and the
    /// surplus rows were paired in read order — a best effort that has to be
    /// visible, because it can turn a reordering into a wall of changes.
    pub duplicate_keys_a: Vec<RowKey>,
    pub duplicate_keys_b: Vec<RowKey>,
    /// At least one list was cut short by the cap. The `*_total` fields are still
    /// exact.
    pub truncated: bool,
}

impl RowsComparison {
    pub fn has_differences(&self) -> bool {
        self.only_in_a_total > 0
            || self.only_in_b_total > 0
            || self.changed_total > 0
            || !self.columns_only_in_a.is_empty()
            || !self.columns_only_in_b.is_empty()
    }
}

/// Compare two sets of rows.
///
/// Errors only on a question that cannot be asked — a key column that does not
/// exist, a row that does not fit its header. A difference is never an error.
pub fn compare_rows(
    label: &str,
    a: &RowSet,
    b: &RowSet,
    options: &RowCompareOptions,
) -> Result<RowsComparison, DiffError> {
    check_widths(label, a, Side::A)?;
    check_widths(label, b, Side::B)?;

    let ci = options.case_insensitive_names;
    let compared = resolve_columns(label, a, b, options)?;
    let key = resolve_key(label, a, b, options)?;

    let mut out = RowsComparison {
        label: label.to_string(),
        key: options.key.clone(),
        keyed: !key.is_empty(),
        compared_columns: compared.iter().map(|c| c.name.clone()).collect(),
        columns_only_in_a: missing_from(&a.columns, &b.columns, ci),
        columns_only_in_b: missing_from(&b.columns, &a.columns, ci),
        rows_a: a.rows.len(),
        rows_b: b.rows.len(),
        ..Default::default()
    };

    let mut sink = Sink { cap: options.max_differences, used: 0 };
    if key.is_empty() {
        compare_positionally(a, b, &compared, &mut out, &mut sink);
    } else {
        compare_by_key(a, b, &compared, &key, &mut out, &mut sink);
    }
    Ok(out)
}

/// A column that exists on both sides, with its index in each.
#[derive(Debug, Clone)]
pub(crate) struct ColumnPair {
    pub(crate) name: String,
    pub(crate) ia: usize,
    pub(crate) ib: usize,
}

/// Budget for what gets **listed**. Counting is never capped.
#[derive(Debug)]
pub(crate) struct Sink {
    cap: Option<usize>,
    used: usize,
}

impl Sink {
    pub(crate) fn room(&mut self) -> bool {
        match self.cap {
            Some(c) if self.used >= c => false,
            _ => {
                self.used += 1;
                true
            }
        }
    }
}

fn check_widths(label: &str, set: &RowSet, side: Side) -> Result<(), DiffError> {
    let expected = set.columns.len();
    match set.rows.iter().position(|r| r.len() != expected) {
        None => Ok(()),
        Some(row) => Err(DiffError::RowWidthMismatch {
            label: label.to_string(),
            side,
            row,
            expected,
            found: set.rows[row].len(),
        }),
    }
}

/// The columns to compare: the explicit list if there is one, otherwise
/// everything both sides have, minus the ignored globs.
fn resolve_columns(
    label: &str,
    a: &RowSet,
    b: &RowSet,
    options: &RowCompareOptions,
) -> Result<Vec<ColumnPair>, DiffError> {
    let ci = options.case_insensitive_names;
    let wanted: Vec<String> =
        if options.columns.is_empty() { a.columns.clone() } else { options.columns.clone() };
    let explicit = !options.columns.is_empty();

    let mut out = Vec::with_capacity(wanted.len());
    for name in wanted {
        if matches_any(&options.ignore_columns, &name, ci) {
            continue;
        }
        let ia = a.column_index(&name, ci);
        let ib = b.column_index(&name, ci);
        match (ia, ib) {
            (Some(ia), Some(ib)) => out.push(ColumnPair { name, ia, ib }),
            // A column the user asked for by name and one side does not have is a
            // mistake worth stopping on. One that simply is not on both sides
            // shows up as `columnsOnlyIn*` and the rest of the row still compares.
            (None, _) if explicit => {
                return Err(DiffError::MissingColumn {
                    label: label.to_string(),
                    column: name,
                    side: Side::A,
                })
            }
            (_, None) if explicit => {
                return Err(DiffError::MissingColumn {
                    label: label.to_string(),
                    column: name,
                    side: Side::B,
                })
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The key columns, or an empty vector for a positional comparison.
fn resolve_key(
    label: &str,
    a: &RowSet,
    b: &RowSet,
    options: &RowCompareOptions,
) -> Result<Vec<ColumnPair>, DiffError> {
    let ci = options.case_insensitive_names;
    let mut out = Vec::with_capacity(options.key.len());
    for name in &options.key {
        let missing = |side| DiffError::MissingKeyColumn {
            label: label.to_string(),
            column: name.clone(),
            side,
        };
        let ia = a.column_index(name, ci).ok_or_else(|| missing(Side::A))?;
        let ib = b.column_index(name, ci).ok_or_else(|| missing(Side::B))?;
        out.push(ColumnPair { name: name.clone(), ia, ib });
    }
    Ok(out)
}
