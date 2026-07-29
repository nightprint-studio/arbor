//! Procedural blocks — the wrapper that makes guards possible.
//!
//! A guard is an early `RETURN`, and there is nowhere to return from in a bare
//! statement. So "run only from version 4.12", "skip rows already there" and "bail
//! out if the table isn't there" all require a block, and the two engines write one
//! very differently: `DECLARE … BEGIN … END; /` against `DO $$ … END $$;`.
//!
//! The generated identifiers (`before_changes`, `v_version`, …) are English, like
//! the rest of Arbor's code.

use picus_ast::prelude::{DialectScope, DmlModel, DmlOperation, EngineKind, Target};

use crate::literal::{ident, literal};
use crate::statement::{statement_for, EmitResult};

const INDENT: &str = "    ";

/// Savepoint name for a transactional target.
const SAVEPOINT: &str = "before_changes";

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("{INDENT}{l}")).collect::<Vec<_>>().join("\n")
}

/// `WHERE …` selecting the version row this destination reads and stamps; empty
/// when the table holds a single row.
///
/// Asked of the **target**, not of the model, because a repository that installs
/// several products keeps a row per product and one generation writes into more
/// than one of them. `Target::version_predicate` owns the precedence, so the four
/// call sites below cannot disagree about it.
fn version_filter(model: &DmlModel, target: &Target) -> String {
    let f = target.version_predicate(model).trim();
    if f.is_empty() {
        String::new()
    } else {
        format!("\n   WHERE {f}")
    }
}

/// Emit the block for one target. Dispatches on the **target's** scope.
///
/// A portable target never reaches here — `Target::rule_conflict` refuses the
/// combination, because the two engines spell a procedural block with constructs
/// the other cannot parse and there is no third spelling. The `None` arm says so
/// rather than picking one, which is the same refusal the caller already got.
pub fn block(model: &DmlModel, target: &Target) -> EmitResult {
    match target.dialect.dialect() {
        Some(EngineKind::Oracle) => oracle_block(model, target),
        Some(EngineKind::Postgres) => postgres_block(model, target),
        None => Err(PORTABLE_BLOCK),
    }
}

const PORTABLE_BLOCK: &str = "a portable script cannot use a procedural block: Oracle spells \n    it `DECLARE … BEGIN … END; /` and PostgreSQL `DO $$ … $$`, and no form runs on both";

fn oracle_block(model: &DmlModel, target: &Target) -> EmitResult {
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
            version_filter(model, target),
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
        let body = indent(&statement_for(model, row, DialectScope::One(EngineKind::Oracle), target.operation_for(model.operation))?);
        // A delete is skip-if-present's own no-op: deleting a row that isn't there
        // already does nothing, so guarding it would only add noise.
        if g.skip_if_present && model.operation != DmlOperation::Delete {
            let predicate = model
                .key_columns
                .iter()
                .map(|c| format!("{} = {}", c.name, literal(row.get(&c.name).map(String::as_str), c, DialectScope::One(EngineKind::Oracle))))
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
            version_filter(model, target)
        ));
    }

    out.push_str("  COMMIT;\n");
    if g.transactional {
        out.push_str(&format!(
            "EXCEPTION\n  WHEN OTHERS THEN\n    ROLLBACK TO {SAVEPOINT};\n    RAISE;\n"
        ));
    }
    out.push_str("END;\n/");
    Ok(out)
}

fn postgres_block(model: &DmlModel, target: &Target) -> EmitResult {
    let g = &target.guards;
    let lc = model.lowercase_postgres;
    let v = &model.version_table;
    let table = ident(&model.table, DialectScope::One(EngineKind::Postgres), lc);
    let v_table = ident(&v.table, DialectScope::One(EngineKind::Postgres), lc);
    let v_column = ident(&v.version_column, DialectScope::One(EngineKind::Postgres), lc);

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
            version_filter(model, target),
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
        let body = indent(&statement_for(model, row, DialectScope::One(EngineKind::Postgres), target.operation_for(model.operation))?);
        if g.skip_if_present && model.operation != DmlOperation::Delete {
            let predicate = model
                .key_columns
                .iter()
                .map(|c| {
                    format!(
                        "{} = {}",
                        ident(&c.name, DialectScope::One(EngineKind::Postgres), lc),
                        literal(row.get(&c.name).map(String::as_str), c, DialectScope::One(EngineKind::Postgres))
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
            sets.push(format!("{} = CURRENT_TIMESTAMP", ident(date, DialectScope::One(EngineKind::Postgres), lc)));
        }
        out.push_str(&format!(
            "\n  -- carry the database to {}\n  UPDATE {v_table} SET {}{};\n",
            guard.to,
            sets.join(", "),
            version_filter(model, target)
        ));
    }

    // No COMMIT: a DO block runs inside the caller's transaction, and PostgreSQL
    // refuses a COMMIT there. The Oracle side commits because its block is the
    // transaction. Same rule, two correct spellings — which is the product.
    out.push_str("END $$;");
    Ok(out)
}
