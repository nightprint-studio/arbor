//! [`DmlModel`] — what to write, with no engine attached.
//!
//! This is the hinge of the whole product. A change is described **once**, in a
//! form that mentions no dialect: a table, an operation, some rows, a comparison
//! key. Emission then turns that one description into as many correct statements
//! as there are destinations — the Oracle branch and the PostgreSQL branch, each in
//! its own syntax.
//!
//! Which is why nothing here has an engine field. If a dialect leaked into this
//! model, "the same change in both branches" would stop being a guarantee and
//! become a coincidence.

use picus_types::prelude::Column;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the change does to the rows it describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DmlOperation {
    Insert,
    /// Insert the row if the comparison key isn't there, update it if it is. The
    /// engines spell this very differently, which is the point of describing it
    /// by intent rather than by syntax.
    Upsert,
    Update,
    Delete,
    /// Delete the row by its comparison key, then insert it — "make the row be
    /// exactly this, whatever it was".
    ///
    /// The shape an **update script** needs when the row is already installed. An
    /// upsert says the same thing more elegantly, but it is the one operation with
    /// no portable spelling and, on Oracle, a `MERGE` that reads nothing like the
    /// hand-written scripts it sits among. Two plain statements are what these
    /// repositories already write by hand, they run on both engines, and they are
    /// re-runnable: the `DELETE` is a no-op the first time.
    ///
    /// Deliberately **not** the same as `Delete` followed by `Insert` as two
    /// generations: it is one intention, so it is one operation, one block and one
    /// marker.
    Replace,
}

/// One row of values, keyed by column name.
///
/// A missing key and an empty string mean the same thing — *not supplied* — and a
/// column that wasn't supplied is left out of the statement entirely rather than
/// written as `NULL`. Those are different intentions and Picus keeps them apart.
pub type DmlRow = BTreeMap<String, String>;

/// Where the installed version is recorded, and what to stamp on upgrade.
///
/// Every project records its version somewhere; almost none agree on how. Making
/// this configuration rather than constants is what lets the version guard — the
/// single most valuable rule Picus has — work outside the one project it was first
/// modelled on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionTableConfig {
    /// Table holding the installed version. Empty disables version guards.
    pub table: String,
    /// Column holding the version string.
    pub version_column: String,
    /// Column stamped with the moment of the upgrade. `None` when the project
    /// doesn't track one — the emitter then leaves it out of the UPDATE entirely
    /// rather than inventing a column that would fail on the first run.
    #[serde(default)]
    pub date_column: Option<String>,
    /// Extra predicate for version tables holding one row per module
    /// (`MODULE = 'CORE'`). Empty means the table holds a single row.
    #[serde(default)]
    pub filter: String,
}

impl Default for VersionTableConfig {
    fn default() -> Self {
        Self {
            table: "VERSIONE_DB".to_string(),
            version_column: "VERSIONE".to_string(),
            date_column: Some("DATA_AGG".to_string()),
            filter: String::new(),
        }
    }
}

/// The dialect-free description of a change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DmlModel {
    pub table: String,
    pub operation: DmlOperation,
    /// The table's full column set — drives value formatting (a numeric column's
    /// value is emitted bare) and column ordering.
    pub columns: Vec<Column>,
    /// The comparison key: the `WHERE` of an update or delete, the conflict target
    /// of an upsert, and the existence test of a skip-if-present guard.
    pub key_columns: Vec<Column>,
    pub rows: Vec<DmlRow>,
    /// Lowercase identifiers when emitting PostgreSQL. A per-project convention,
    /// never applied to Oracle.
    #[serde(default)]
    pub lowercase_postgres: bool,
    /// Where the installed version lives — per project, never a constant.
    #[serde(default)]
    pub version_table: VersionTableConfig,
}

impl DmlModel {
    /// The columns actually supplied in a row, in the table's column order.
    ///
    /// An omitted column is left out of the statement rather than written as
    /// `NULL`: "I didn't set this" and "I set this to NULL" are different
    /// intentions, and in a script that runs against a live database the
    /// difference is a column default silently lost.
    pub fn supplied_columns<'a>(&'a self, row: &DmlRow) -> Vec<&'a Column> {
        self.columns
            .iter()
            .filter(|c| row.get(&c.name).map(|v| !v.trim().is_empty()).unwrap_or(false))
            .collect()
    }

    /// The supplied columns that are not part of the comparison key — what an
    /// update assigns.
    pub fn non_key_columns<'a>(&'a self, row: &DmlRow) -> Vec<&'a Column> {
        self.supplied_columns(row)
            .into_iter()
            .filter(|c| !self.key_columns.iter().any(|k| k.name == c.name))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, ty: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            primary_key: false,
            not_null: false,
            default_value: None,
        }
    }

    fn model() -> DmlModel {
        DmlModel {
            table: "PARAMETRI".to_string(),
            operation: DmlOperation::Insert,
            columns: vec![column("CODE", "varchar(10)"), column("VALUE", "numeric"), column("NOTE", "text")],
            key_columns: vec![column("CODE", "varchar(10)")],
            rows: vec![],
            lowercase_postgres: false,
            version_table: VersionTableConfig::default(),
        }
    }

    #[test]
    fn an_omitted_column_is_not_supplied() {
        let mut row = DmlRow::new();
        row.insert("CODE".into(), "A".into());
        row.insert("VALUE".into(), "3".into());
        // NOTE deliberately absent: it must be left out of the statement, not
        // written as NULL, so the column's default still applies.
        let m = model();
        let names: Vec<&str> = m.supplied_columns(&row).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["CODE", "VALUE"]);
    }

    #[test]
    fn a_blank_value_counts_as_not_supplied() {
        let mut row = DmlRow::new();
        row.insert("CODE".into(), "A".into());
        row.insert("NOTE".into(), "   ".into());
        let m = model();
        let names: Vec<&str> = m.supplied_columns(&row).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["CODE"]);
    }

    #[test]
    fn supplied_columns_keep_the_tables_order_not_the_rows() {
        let mut row = DmlRow::new();
        // Inserted in reverse; the map is sorted by key, and neither order is the
        // table's. The statement must follow the TABLE.
        row.insert("NOTE".into(), "n".into());
        row.insert("VALUE".into(), "1".into());
        row.insert("CODE".into(), "A".into());
        let m = model();
        let names: Vec<&str> = m.supplied_columns(&row).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["CODE", "VALUE", "NOTE"]);
    }

    #[test]
    fn the_key_is_excluded_from_what_an_update_assigns() {
        let mut row = DmlRow::new();
        row.insert("CODE".into(), "A".into());
        row.insert("VALUE".into(), "3".into());
        let m = model();
        let names: Vec<&str> = m.non_key_columns(&row).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["VALUE"]);
    }

    #[test]
    fn a_version_table_without_a_date_column_round_trips_as_such() {
        let v = VersionTableConfig {
            table: "APP_VERSION".into(),
            version_column: "V".into(),
            date_column: None,
            filter: String::new(),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: VersionTableConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.date_column, None, "'no date column' must survive the wire");
    }
}
