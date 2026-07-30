//! Behavioural tests, one module per thing that can be wrong.
//!
//! Everything here runs against fixtures, which is the point of the crate being
//! pure: a composite key, a `1` that is not a `1.0` and a threshold that has just
//! been crossed are all cases that would need a database to reproduce, and none
//! of them do here.
//!
//! The fixtures use invented, neutral names on purpose — no table, column or
//! constraint from any real repository appears in this crate.

mod config_filters;
mod counts;
mod names;
mod report;
mod rows;
mod schema;
mod template;

use picus_types::prelude::{
    Column, ForeignKey, IndexInfo, RelationKind, SchemaSnapshot, SequenceInfo, TableInfo,
    TriggerInfo,
};

pub(crate) fn column(name: &str, data_type: &str) -> Column {
    Column {
        name: name.to_string(),
        data_type: data_type.to_string(),
        primary_key: false,
        not_null: false,
        default_value: None,
    }
}

pub(crate) fn key_column(name: &str, data_type: &str) -> Column {
    Column { primary_key: true, not_null: true, ..column(name, data_type) }
}

pub(crate) fn table(name: &str, columns: Vec<Column>) -> TableInfo {
    TableInfo {
        name: name.to_string(),
        kind: RelationKind::Table,
        columns,
        primary_key_name: None,
        foreign_keys: None,
        indexes: None,
        definition: None,
        estimated_rows: None,
    }
}

pub(crate) fn view(name: &str, definition: &str) -> TableInfo {
    TableInfo {
        kind: RelationKind::View,
        definition: Some(definition.to_string()),
        ..table(name, vec![column("code", "text")])
    }
}

pub(crate) fn index(name: &str, columns: &[&str], unique: bool) -> IndexInfo {
    IndexInfo {
        name: name.to_string(),
        columns: columns.iter().map(|c| c.to_string()).collect(),
        unique,
        kind: None,
        primary_key: false,
    }
}

pub(crate) fn foreign_key(name: &str, column: &str, to: &str, to_column: &str) -> ForeignKey {
    ForeignKey {
        name: name.to_string(),
        columns: vec![column.to_string()],
        referenced_table: to.to_string(),
        referenced_columns: vec![to_column.to_string()],
        on_delete: None,
    }
}

pub(crate) fn sequence(name: &str, last_value: i64) -> SequenceInfo {
    SequenceInfo {
        name: name.to_string(),
        last_value,
        increment_by: 1,
        min_value: None,
        max_value: None,
        cycle: false,
        cache_size: None,
    }
}

pub(crate) fn trigger(name: &str, table: &str, events: &[&str]) -> TriggerInfo {
    TriggerInfo {
        name: name.to_string(),
        table: table.to_string(),
        timing: "BEFORE".to_string(),
        events: events.iter().map(|e| e.to_string()).collect(),
        enabled: true,
        for_each_row: true,
    }
}

pub(crate) fn snapshot(tables: Vec<TableInfo>) -> SchemaSnapshot {
    SchemaSnapshot { tables, views: Vec::new(), sequences: Vec::new(), triggers: Vec::new() }
}
