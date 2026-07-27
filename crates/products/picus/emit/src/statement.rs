//! One statement, no wrapper.
//!
//! Where the dialects visibly diverge: an upsert is `MERGE … FROM DUAL` on Oracle
//! and `INSERT … ON CONFLICT` on PostgreSQL. Same intent, described once in the
//! model, spelled differently here — which is the whole reason the model carries no
//! dialect.

use picus_ast::prelude::{DmlModel, DmlOperation, DmlRow, EngineKind};

use crate::literal::{ident, literal};

/// Emit one row as a single statement in `dialect`.
pub fn plain_statement(model: &DmlModel, row: &DmlRow, dialect: EngineKind) -> String {
    let lc = model.lowercase_postgres;
    let table = ident(&model.table, dialect, lc);
    let cols = model.supplied_columns(row);
    let non_key = model.non_key_columns(row);
    let keys = &model.key_columns;

    let id = |name: &str| ident(name, dialect, lc);
    let val = |name: &str| {
        let column = model
            .columns
            .iter()
            .find(|c| c.name == name)
            .or_else(|| keys.iter().find(|c| c.name == name));
        match column {
            Some(c) => literal(row.get(name).map(String::as_str), c, dialect),
            None => "NULL".to_string(),
        }
    };

    let col_list = |list: &[&picus_types::prelude::Column]| {
        list.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", ")
    };
    let val_list =
        |list: &[&picus_types::prelude::Column]| list.iter().map(|c| val(&c.name)).collect::<Vec<_>>().join(", ");
    let key_predicate = |sep: &str| {
        keys.iter().map(|c| format!("{} = {}", id(&c.name), val(&c.name))).collect::<Vec<_>>().join(sep)
    };

    match model.operation {
        DmlOperation::Insert => format!(
            "INSERT INTO {table} ({})\nVALUES ({});",
            col_list(&cols),
            val_list(&cols)
        ),

        DmlOperation::Update => format!(
            "UPDATE {table} SET {}\n WHERE {};",
            non_key
                .iter()
                .map(|c| format!("{} = {}", id(&c.name), val(&c.name)))
                .collect::<Vec<_>>()
                .join(", "),
            key_predicate(" AND ")
        ),

        DmlOperation::Delete => {
            format!("DELETE FROM {table}\n WHERE {};", key_predicate(" AND "))
        }

        DmlOperation::Upsert => match dialect {
            EngineKind::Postgres => format!(
                "INSERT INTO {table} ({})\nVALUES ({})\nON CONFLICT ({}) DO UPDATE\n   SET {};",
                col_list(&cols),
                val_list(&cols),
                keys.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", "),
                non_key
                    .iter()
                    .map(|c| format!("{} = EXCLUDED.{}", id(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            EngineKind::Oracle => format!(
                "MERGE INTO {table} d\nUSING (SELECT {} FROM DUAL) s\n   ON ({})\nWHEN MATCHED THEN UPDATE SET {}\nWHEN NOT MATCHED THEN INSERT ({}) VALUES ({});",
                keys.iter()
                    .map(|c| format!("{} AS {}", val(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", "),
                keys.iter()
                    .map(|c| format!("d.{} = s.{}", id(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(" AND "),
                non_key
                    .iter()
                    .map(|c| format!("d.{} = {}", id(&c.name), val(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", "),
                col_list(&cols),
                val_list(&cols)
            ),
        },
    }
}
