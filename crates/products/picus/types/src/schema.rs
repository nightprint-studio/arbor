//! The schema wire types.
//!
//! These are serialised **camelCase** on purpose: they are the exact shapes
//! `src/lib/types/picus/index.ts` already renders, so the frontend binds a real
//! backend without a translation layer in the middle. When one of these changes,
//! the TypeScript interface changes in the same turn.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    /// Native type exactly as the server reports it (`character varying(30)`),
    /// never normalised — the user is maintaining scripts for *this* server.
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary_key: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_null: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// A referential constraint, with what happens to the child rows on delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    /// Server-side index kind as reported (`btree`, `gin`, `BITMAP`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// True for the index backing the primary key — not an object the user
    /// created, and not one they can drop.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary_key: bool,
}

/// What sort of relation this is. Views carry a definition instead of constraints;
/// everything else about them reads the same, which is why they share a shape
/// rather than a hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub kind: RelationKind,
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_key_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_keys: Option<Vec<ForeignKey>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexes: Option<Vec<IndexInfo>>,
    /// Views only: the SELECT the view is defined as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// Approximate row count when the server can give one cheaply. `None` rather
    /// than `0`: "unknown" and "empty" must not render the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_rows: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub name: String,
    pub last_value: i64,
    pub increment_by: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
    pub cycle: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_size: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    /// Table the trigger is attached to.
    pub table: String,
    /// `BEFORE` | `AFTER` | `INSTEAD OF`.
    pub timing: String,
    /// `INSERT` / `UPDATE` / `DELETE` — a trigger can answer to several.
    pub events: Vec<String>,
    pub enabled: bool,
    /// Row-level vs statement-level.
    pub for_each_row: bool,
}

/// One connection's schema, as far as it has been read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSnapshot {
    pub tables: Vec<TableInfo>,
    pub views: Vec<TableInfo>,
    pub sequences: Vec<SequenceInfo>,
    pub triggers: Vec<TriggerInfo>,
}

/// serde skip helper — keeps the wire payload small for the many false flags on a
/// wide table.
fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_serialises_to_the_frontend_shape() {
        let c = Column {
            name: "ID".into(),
            data_type: "integer".into(),
            primary_key: true,
            not_null: true,
            default_value: Some("nextval('s')".into()),
        };
        let v = serde_json::to_value(&c).unwrap();
        // The frontend's `Column` reads exactly these keys.
        assert_eq!(v["name"], "ID");
        assert_eq!(v["type"], "integer");
        assert_eq!(v["primaryKey"], true);
        assert_eq!(v["notNull"], true);
        assert_eq!(v["defaultValue"], "nextval('s')");
    }

    #[test]
    fn false_flags_and_absent_options_are_omitted() {
        let c = Column {
            name: "NOTE".into(),
            data_type: "text".into(),
            primary_key: false,
            not_null: false,
            default_value: None,
        };
        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("primaryKey").is_none());
        assert!(v.get("notNull").is_none());
        assert!(v.get("defaultValue").is_none());
    }

    #[test]
    fn unknown_row_count_is_not_zero() {
        let t = TableInfo {
            name: "T".into(),
            kind: RelationKind::Table,
            columns: vec![],
            primary_key_name: None,
            foreign_keys: None,
            indexes: None,
            definition: None,
            estimated_rows: None,
        };
        let v = serde_json::to_value(&t).unwrap();
        assert!(v.get("estimatedRows").is_none(), "'unknown' must not render as 'empty'");
        assert_eq!(v["kind"], "table");
    }
}
