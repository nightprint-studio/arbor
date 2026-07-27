//! Procedural blocks — the wrapper that makes guards possible.
//!
//! A guard is an early `RETURN`, and there is nowhere to return from in a bare
//! statement. So "run only from version 4.12", "skip rows already there" and "bail
//! out if the table isn't there" all require a block, and the two engines write one
//! very differently: `DECLARE … BEGIN … END; /` against `DO $$ … END $$;`.
//!
//! The generated identifiers (`before_changes`, `v_version`, …) are English, like
//! the rest of Arbor's code.

use picus_ast::prelude::{DmlModel, DmlOperation, EngineKind, Target, VersionTableConfig};

use crate::literal::{ident, literal};
use crate::statement::plain_statement;

const INDENT: &str = "    ";

/// Savepoint name for a transactional target.
const SAVEPOINT: &str = "before_changes";

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("{INDENT}{l}")).collect::<Vec<_>>().join("\n")
}

/// `WHERE …` for version tables holding one row per module; empty otherwise.
fn version_filter(v: &VersionTableConfig) -> String {
    let f = v.filter.trim();
    if f.is_empty() {
        String::new()
    } else {
        format!("\n   WHERE {f}")
    }
}

/// Emit the block for one target. Dispatches on the **target's** dialect.
pub fn block(model: &DmlModel, target: &Target) -> String {
    match target.dialect {
        EngineKind::Oracle => oracle_block(model, target),
        EngineKind::Postgres => postgres_block(model, target),
    }
}

fn oracle_block(model: &DmlModel, target: &Target) -> String {
    let g = &target.guards;
    let v = &model.version_table;
    let mut out = String::from("DECLARE\n");

    if g.version.is_some() {
        out.push_str("  v_version VARCHAR2(30);\n");
    }
    if g.skip_if_present {
        out.push_str("  v_existing NUMBER;\n");
    }
    if g.require_object {
        out.push_str("  v_object   NUMBER;\n");
    }
    out.push_str("BEGIN\n");

    if let Some(guard) = &g.version {
        out.push_str(&format!(
            "  -- guard: only applies when starting from {}\n  SELECT {} INTO v_version FROM {}{};\n  IF v_version <> '{}' THEN\n    RETURN;\n  END IF;\n\n",
            guard.from,
            v.version_column,
            v.table,
            version_filter(v),
            guard.from
        ));
    }
    if g.require_object {
        out.push_str(&format!(
            "  SELECT COUNT(*) INTO v_object FROM USER_TABLES WHERE TABLE_NAME = '{}';\n  IF v_object = 0 THEN\n    RETURN;\n  END IF;\n\n",
            model.table
        ));
    }
    if g.transactional {
        out.push_str(&format!("  SAVEPOINT {SAVEPOINT};\n\n"));
    }

    let last = model.rows.len().saturating_sub(1);
    for (i, row) in model.rows.iter().enumerate() {
        let body = indent(&plain_statement(model, row, EngineKind::Oracle));
        // A delete is skip-if-present's own no-op: deleting a row that isn't there
        // already does nothing, so guarding it would only add noise.
        if g.skip_if_present && model.operation != DmlOperation::Delete {
            let predicate = model
                .key_columns
                .iter()
                .map(|c| format!("{} = {}", c.name, literal(row.get(&c.name).map(String::as_str), c, EngineKind::Oracle)))
                .collect::<Vec<_>>()
                .join(" AND ");
            out.push_str(&format!(
                "  SELECT COUNT(*) INTO v_existing FROM {}\n   WHERE {predicate};\n  IF v_existing = 0 THEN\n{body}\n  END IF;\n",
                model.table
            ));
        } else {
            out.push_str(&format!("{body}\n"));
        }
        if i < last {
            out.push('\n');
        }
    }

    if let Some(guard) = &g.version {
        // The date column is stamped ONLY when the project has one. Plenty of
        // version tables hold nothing but the version string, and inventing a
        // column emits an UPDATE that fails on the first run.
        let mut sets = vec![format!("{} = '{}'", v.version_column, guard.to)];
        if let Some(date) = &v.date_column {
            sets.push(format!("{date} = SYSDATE"));
        }
        out.push_str(&format!(
            "\n  -- carry the database to {}\n  UPDATE {} SET {}{};\n",
            guard.to,
            v.table,
            sets.join(", "),
            version_filter(v)
        ));
    }

    out.push_str("  COMMIT;\n");
    if g.transactional {
        out.push_str(&format!(
            "EXCEPTION\n  WHEN OTHERS THEN\n    ROLLBACK TO {SAVEPOINT};\n    RAISE;\n"
        ));
    }
    out.push_str("END;\n/");
    out
}

fn postgres_block(model: &DmlModel, target: &Target) -> String {
    let g = &target.guards;
    let lc = model.lowercase_postgres;
    let v = &model.version_table;
    let table = ident(&model.table, EngineKind::Postgres, lc);
    let v_table = ident(&v.table, EngineKind::Postgres, lc);
    let v_column = ident(&v.version_column, EngineKind::Postgres, lc);

    let mut out = String::from("DO $$\n");
    if g.version.is_some() || g.skip_if_present {
        out.push_str("DECLARE\n");
        if g.version.is_some() {
            out.push_str("  v_version text;\n");
        }
        if g.skip_if_present {
            out.push_str("  v_existing int;\n");
        }
    }
    out.push_str("BEGIN\n");

    if let Some(guard) = &g.version {
        out.push_str(&format!(
            "  -- guard: only applies when starting from {}\n  SELECT {v_column} INTO v_version FROM {v_table}{};\n  IF v_version <> '{}' THEN\n    RETURN;\n  END IF;\n\n",
            guard.from,
            version_filter(v),
            guard.from
        ));
    }
    if g.require_object {
        out.push_str(&format!(
            "  IF to_regclass('{table}') IS NULL THEN\n    RETURN;\n  END IF;\n\n"
        ));
    }

    let last = model.rows.len().saturating_sub(1);
    for (i, row) in model.rows.iter().enumerate() {
        let body = indent(&plain_statement(model, row, EngineKind::Postgres));
        if g.skip_if_present && model.operation != DmlOperation::Delete {
            let predicate = model
                .key_columns
                .iter()
                .map(|c| {
                    format!(
                        "{} = {}",
                        ident(&c.name, EngineKind::Postgres, lc),
                        literal(row.get(&c.name).map(String::as_str), c, EngineKind::Postgres)
                    )
                })
                .collect::<Vec<_>>()
                .join(" AND ");
            out.push_str(&format!(
                "  SELECT count(*) INTO v_existing FROM {table}\n   WHERE {predicate};\n  IF v_existing = 0 THEN\n{body}\n  END IF;\n"
            ));
        } else {
            out.push_str(&format!("{body}\n"));
        }
        if i < last {
            out.push('\n');
        }
    }

    if let Some(guard) = &g.version {
        let mut sets = vec![format!("{v_column} = '{}'", guard.to)];
        if let Some(date) = &v.date_column {
            sets.push(format!("{} = CURRENT_TIMESTAMP", ident(date, EngineKind::Postgres, lc)));
        }
        out.push_str(&format!(
            "\n  -- carry the database to {}\n  UPDATE {v_table} SET {}{};\n",
            guard.to,
            sets.join(", "),
            version_filter(v)
        ));
    }

    // No COMMIT: a DO block runs inside the caller's transaction, and PostgreSQL
    // refuses a COMMIT there. The Oracle side commits because its block is the
    // transaction. Same rule, two correct spellings — which is the product.
    out.push_str("END $$;");
    out
}
