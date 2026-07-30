//! The three abbreviations that have no single spelling, written for one engine.
//!
//! `arbor-sql-abbrev` resolves `m#`, `a#` and `fc#` into an intent and renders a
//! standard form for hosts with no engine. Picus has one, so it renders them here
//! — the same split as `i#`/`u#`, and for the same reason: **every dialect fact in
//! this product lives on one side of one seam**.
//!
//! What the three differ about:
//!
//! | | PostgreSQL | Oracle |
//! |---|---|---|
//! | upsert | `INSERT … ON CONFLICT (k) DO UPDATE` | `MERGE … USING (SELECT … FROM DUAL)` |
//! | add a column | `ADD COLUMN nota varchar(200)` | `ADD (NOTA VARCHAR2(200))` |
//! | retype one | `ALTER COLUMN x TYPE numeric(12,2)` | `MODIFY (X NUMBER(12,2))` |
//! | cursor loop | `FOR r IN SELECT … LOOP` | `FOR r IN (SELECT …) LOOP` |
//!
//! The last row is the one that looks like pedantry and is not: PL/pgSQL **rejects**
//! the parentheses PL/SQL conventionally uses, so a single spelling would be wrong
//! on one of the two engines every time.

use arbor_sql_abbrev::prelude::{
    render, ChangeKind, ColumnChange, ColumnRef, RenderStyle, Statement,
};
use picus_ast::prelude::{DialectScope, EngineKind};
use picus_emit::prelude::{ident, PORTABLE_UPSERT};

/// `m#` — the upsert skeleton, with a named parameter per column.
///
/// Parameters rather than literals because a merge written in nine characters has
/// no values in it: the user is asking for the shape, and `:codice` says which
/// column each hole belongs to in a way `?` cannot.
pub fn merge_sql(
    table: &str,
    columns: &[ColumnRef],
    keys: &[ColumnRef],
    scope: DialectScope,
) -> Result<String, String> {
    let id = |name: &str| ident(name, scope, false);
    let is_key = |c: &ColumnRef| keys.iter().any(|k| k.name == c.name);
    let updated: Vec<&ColumnRef> = columns.iter().filter(|c| !is_key(c)).collect();
    let names = columns.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", ");

    match scope.dialect() {
        // No portable arm, because there is no portable upsert. The wording is the
        // emitter's own, byte for byte: a user may meet this refusal from the
        // generator or from a line they are typing, and two spellings of one rule
        // read as two rules.
        None => Err(PORTABLE_UPSERT.to_string()),

        Some(EngineKind::Postgres) => Ok(format!(
            "INSERT INTO {} ({names})\nVALUES ({})\nON CONFLICT ({}) DO UPDATE SET\n{};",
            id(table),
            columns.iter().map(|c| format!(":{}", c.name)).collect::<Vec<_>>().join(", "),
            keys.iter().map(|c| id(&c.name)).collect::<Vec<_>>().join(", "),
            updated
                .iter()
                .map(|c| format!("      {} = EXCLUDED.{}", id(&c.name), id(&c.name)))
                .collect::<Vec<_>>()
                .join(",\n")
        )),

        Some(EngineKind::Oracle) => Ok([
            format!("MERGE INTO {} d", id(table)),
            format!(
                "USING (SELECT {} FROM dual) s",
                columns
                    .iter()
                    .map(|c| format!(":{} AS {}", c.name, id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!(
                "   ON ({})",
                keys.iter()
                    .map(|c| format!("d.{} = s.{}", id(&c.name), id(&c.name)))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
            " WHEN MATCHED THEN UPDATE SET".to_string(),
            updated
                .iter()
                .map(|c| format!("      d.{} = s.{}", id(&c.name), id(&c.name)))
                .collect::<Vec<_>>()
                .join(",\n"),
            format!(" WHEN NOT MATCHED THEN INSERT ({names})"),
            format!(
                "      VALUES ({});",
                columns.iter().map(|c| format!("s.{}", id(&c.name))).collect::<Vec<_>>().join(", ")
            ),
        ]
        .join("\n")),
    }
}

/// `a#` — columns added and columns retyped.
///
/// Oracle bundles: `ADD (a …, b …)` is one statement and one table lock, which is
/// how the engine is meant to be driven. PostgreSQL gets one statement per change,
/// which is what its own documentation and everybody's migration files look like.
///
/// A portable folder is refused rather than fudged: the two spellings share no
/// common form, and emitting one of them into a folder that claims to run on both
/// would be a lie the folder is specifically there to prevent.
pub fn alter_sql(
    table: &str,
    changes: &[ColumnChange],
    scope: DialectScope,
) -> Result<String, String> {
    let Some(engine) = scope.dialect() else {
        return Err(PORTABLE_ALTER.to_string());
    };
    let id = |name: &str| ident(name, scope, false);
    let table = id(table);
    let clause = |change: &ColumnChange| {
        format!("{} {}", id(&change.column), type_for(&change.data_type, engine))
    };

    Ok(match engine {
        EngineKind::Postgres => changes
            .iter()
            .map(|change| match change.kind {
                ChangeKind::Add => format!("ALTER TABLE {table} ADD COLUMN {};", clause(change)),
                ChangeKind::Modify => format!(
                    "ALTER TABLE {table} ALTER COLUMN {} TYPE {};",
                    id(&change.column),
                    type_for(&change.data_type, engine)
                ),
            })
            .collect::<Vec<_>>()
            .join("\n"),

        EngineKind::Oracle => {
            let of = |kind: ChangeKind| {
                changes
                    .iter()
                    .filter(|c| c.kind == kind)
                    .map(clause)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            [(ChangeKind::Add, "ADD"), (ChangeKind::Modify, "MODIFY")]
                .into_iter()
                .filter_map(|(kind, word)| {
                    let list = of(kind);
                    (!list.is_empty()).then(|| format!("ALTER TABLE {table} {word} ({list});"))
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    })
}

/// `fc#` — the cursor loop, with the query rendered by the language's own renderer.
///
/// The `SELECT` inside needs no dialect: it is names, joins and comparisons, all of
/// which the two engines spell identically. Only the parentheses round it differ,
/// and only because PL/pgSQL will not accept them.
pub fn for_cursor_sql(
    variable: &str,
    query: &Statement,
    style: &RenderStyle,
    scope: DialectScope,
) -> Result<String, String> {
    let select = render(query, style);
    let source = match scope.dialect() {
        // Portable is legitimate here, and takes the form that parses on both:
        // PL/SQL accepts a bare query, PL/pgSQL only accepts a bare query.
        Some(EngineKind::Oracle) => format!("({select})"),
        _ => select,
    };
    Ok(format!("FOR {variable} IN {source} LOOP\n  NULL; -- TODO\nEND LOOP;"))
}

/// A type as the user wrote it, in the engine's own spelling.
///
/// Portable-in, engine-out: somebody adding a column does not want to remember
/// that `varchar` is `VARCHAR2` here and `varchar` there, and getting it wrong is
/// a statement that fails on one branch of the repository and not the other.
///
/// A name this does not recognise is passed through untouched, deliberately.
/// The alternative is refusing every type nobody thought to list, and a user who
/// spelled it the engine's way was already right.
fn type_for(written: &str, engine: EngineKind) -> String {
    let text = written.trim();
    let (name, args) = match text.split_once('(') {
        Some((name, rest)) => (name.trim(), Some(rest.trim_end_matches(')').trim())),
        None => (text, None),
    };

    let Some(row) = TYPES.iter().find(|r| r.written.eq_ignore_ascii_case(name)) else {
        return text.to_string();
    };
    let (target, default_args) = match engine {
        EngineKind::Postgres => (row.postgres, row.postgres_args),
        EngineKind::Oracle => (row.oracle, row.oracle_args),
    };
    // The written arguments win; the table's are the fallback for the types that
    // are sized on one engine and not on the other (`int` → `NUMBER(10)`).
    match args.filter(|a| !a.is_empty()).or(default_args) {
        Some(args) => format!("{target}({args})"),
        None => target.to_string(),
    }
}

/// One portable type name, and how each engine spells it.
struct TypeRow {
    written: &'static str,
    postgres: &'static str,
    /// Arguments to add when the user wrote none.
    postgres_args: Option<&'static str>,
    oracle: &'static str,
    oracle_args: Option<&'static str>,
}

/// The vocabulary. Short on purpose: these are the types a column actually gets
/// added as, and anything else passes through as typed.
const TYPES: &[TypeRow] = &[
    TypeRow { written: "varchar", postgres: "varchar", postgres_args: None, oracle: "VARCHAR2", oracle_args: None },
    TypeRow { written: "varchar2", postgres: "varchar", postgres_args: None, oracle: "VARCHAR2", oracle_args: None },
    TypeRow { written: "char", postgres: "char", postgres_args: None, oracle: "CHAR", oracle_args: None },
    TypeRow { written: "text", postgres: "text", postgres_args: None, oracle: "CLOB", oracle_args: None },
    TypeRow { written: "clob", postgres: "text", postgres_args: None, oracle: "CLOB", oracle_args: None },
    TypeRow { written: "number", postgres: "numeric", postgres_args: None, oracle: "NUMBER", oracle_args: None },
    TypeRow { written: "numeric", postgres: "numeric", postgres_args: None, oracle: "NUMBER", oracle_args: None },
    TypeRow { written: "decimal", postgres: "numeric", postgres_args: None, oracle: "NUMBER", oracle_args: None },
    // Sized on Oracle because `NUMBER` with no precision is a float there, and a
    // column declared as one stops comparing equal to the integers put in it.
    TypeRow { written: "int", postgres: "integer", postgres_args: None, oracle: "NUMBER", oracle_args: Some("10") },
    TypeRow { written: "integer", postgres: "integer", postgres_args: None, oracle: "NUMBER", oracle_args: Some("10") },
    TypeRow { written: "smallint", postgres: "smallint", postgres_args: None, oracle: "NUMBER", oracle_args: Some("5") },
    TypeRow { written: "bigint", postgres: "bigint", postgres_args: None, oracle: "NUMBER", oracle_args: Some("19") },
    TypeRow { written: "float", postgres: "double precision", postgres_args: None, oracle: "BINARY_DOUBLE", oracle_args: None },
    TypeRow { written: "double", postgres: "double precision", postgres_args: None, oracle: "BINARY_DOUBLE", oracle_args: None },
    // Oracle has no boolean column type — `NUMBER(1)` is what every schema uses.
    TypeRow { written: "bool", postgres: "boolean", postgres_args: None, oracle: "NUMBER", oracle_args: Some("1") },
    TypeRow { written: "boolean", postgres: "boolean", postgres_args: None, oracle: "NUMBER", oracle_args: Some("1") },
    TypeRow { written: "date", postgres: "date", postgres_args: None, oracle: "DATE", oracle_args: None },
    TypeRow { written: "timestamp", postgres: "timestamp", postgres_args: None, oracle: "TIMESTAMP", oracle_args: None },
    TypeRow { written: "blob", postgres: "bytea", postgres_args: None, oracle: "BLOB", oracle_args: None },
    TypeRow { written: "bytea", postgres: "bytea", postgres_args: None, oracle: "BLOB", oracle_args: None },
];

/// Same shape as the upsert's refusal, for the same reason.
const PORTABLE_ALTER: &str = "changing a table has no portable spelling: Oracle writes \
    `ADD (…)` / `MODIFY (…)` and PostgreSQL `ADD COLUMN` / `ALTER COLUMN … TYPE`. Write it \
    into the dialect folders";

#[cfg(test)]
mod tests {
    use super::*;

    const PG: DialectScope = DialectScope::One(EngineKind::Postgres);
    const ORA: DialectScope = DialectScope::One(EngineKind::Oracle);

    fn column(name: &str) -> ColumnRef {
        ColumnRef {
            name: name.to_string(),
            table: "SCHEDARIO".to_string(),
            alias: None,
            kind: arbor_sql_abbrev::prelude::ValueKind::Text,
        }
    }

    fn merge(scope: DialectScope) -> String {
        merge_sql(
            "SCHEDARIO",
            &[column("MATRICOLA"), column("REPARTO"), column("IMPORTO")],
            &[column("MATRICOLA")],
            scope,
        )
        .expect("renders")
    }

    #[test]
    fn postgres_writes_the_upsert_as_on_conflict() {
        let out = merge(PG);
        assert!(out.starts_with("INSERT INTO SCHEDARIO (MATRICOLA, REPARTO, IMPORTO)"), "{out}");
        assert!(out.contains("VALUES (:MATRICOLA, :REPARTO, :IMPORTO)"), "{out}");
        assert!(out.contains("ON CONFLICT (MATRICOLA) DO UPDATE SET"), "{out}");
        assert!(out.contains("REPARTO = EXCLUDED.REPARTO"), "{out}");
        // The key is what was matched on; updating it is the one thing a merge
        // must not do.
        assert!(!out.contains("MATRICOLA = EXCLUDED.MATRICOLA"), "{out}");
    }

    #[test]
    fn oracle_writes_the_upsert_as_a_merge_against_dual() {
        let out = merge(ORA);
        assert!(out.starts_with("MERGE INTO SCHEDARIO d"), "{out}");
        assert!(out.contains("USING (SELECT :MATRICOLA AS MATRICOLA"), "{out}");
        assert!(out.contains("FROM dual) s"), "{out}");
        assert!(out.contains("ON (d.MATRICOLA = s.MATRICOLA)"), "{out}");
        assert!(out.contains("WHEN NOT MATCHED THEN INSERT (MATRICOLA, REPARTO, IMPORTO)"), "{out}");
    }

    #[test]
    fn a_portable_upsert_is_refused_in_the_emitters_own_words() {
        let refusal = merge_sql("SCHEDARIO", &[column("A")], &[column("A")], DialectScope::Portable)
            .expect_err("refused");
        assert_eq!(refusal, PORTABLE_UPSERT);
    }

    fn changes() -> Vec<ColumnChange> {
        vec![
            ColumnChange {
                kind: ChangeKind::Add,
                column: "ANNOTAZIONE".to_string(),
                data_type: "varchar(200)".to_string(),
            },
            ColumnChange {
                kind: ChangeKind::Modify,
                column: "IMPORTO".to_string(),
                data_type: "number(12,2)".to_string(),
            },
        ]
    }

    #[test]
    fn postgres_writes_one_alter_per_change() {
        assert_eq!(
            alter_sql("SCHEDARIO", &changes(), PG).expect("renders"),
            "ALTER TABLE SCHEDARIO ADD COLUMN ANNOTAZIONE varchar(200);\n\
             ALTER TABLE SCHEDARIO ALTER COLUMN IMPORTO TYPE numeric(12,2);"
        );
    }

    #[test]
    fn oracle_bundles_the_adds_and_the_modifies() {
        assert_eq!(
            alter_sql("SCHEDARIO", &changes(), ORA).expect("renders"),
            "ALTER TABLE SCHEDARIO ADD (ANNOTAZIONE VARCHAR2(200));\n\
             ALTER TABLE SCHEDARIO MODIFY (IMPORTO NUMBER(12,2));"
        );
    }

    #[test]
    fn several_columns_of_one_kind_become_one_oracle_clause() {
        let two = vec![
            ColumnChange { kind: ChangeKind::Add, column: "A".into(), data_type: "int".into() },
            ColumnChange { kind: ChangeKind::Add, column: "B".into(), data_type: "date".into() },
        ];
        assert_eq!(
            alter_sql("SCHEDARIO", &two, ORA).expect("renders"),
            "ALTER TABLE SCHEDARIO ADD (A NUMBER(10), B DATE);"
        );
    }

    #[test]
    fn a_type_is_translated_and_an_unknown_one_is_left_alone() {
        assert_eq!(type_for("varchar(30)", EngineKind::Oracle), "VARCHAR2(30)");
        assert_eq!(type_for("VARCHAR2(30)", EngineKind::Postgres), "varchar(30)");
        assert_eq!(type_for("number(12,2)", EngineKind::Postgres), "numeric(12,2)");
        // The two that gain arguments they were not written with, because the
        // engine's unsized form means something else.
        assert_eq!(type_for("int", EngineKind::Oracle), "NUMBER(10)");
        assert_eq!(type_for("boolean", EngineKind::Oracle), "NUMBER(1)");
        assert_eq!(type_for("boolean", EngineKind::Postgres), "boolean");
        // Written arguments always win over the table's.
        assert_eq!(type_for("number(5)", EngineKind::Oracle), "NUMBER(5)");
        // Not in the vocabulary: passed through, because the user who wrote it
        // was already speaking the engine's language.
        assert_eq!(type_for("geometry(Point,4326)", EngineKind::Postgres), "geometry(Point,4326)");
        assert_eq!(type_for("INTERVAL DAY TO SECOND", EngineKind::Oracle), "INTERVAL DAY TO SECOND");
    }

    #[test]
    fn a_portable_alter_is_refused() {
        assert!(alter_sql("SCHEDARIO", &changes(), DialectScope::Portable).is_err());
    }

    #[test]
    fn only_oracle_puts_the_loops_query_in_parentheses() {
        let query = Statement::Delete { table: "X".into(), predicates: vec![] };
        // Any statement will do — this test is about the wrapper, and using a
        // rendered one keeps it independent of what a SELECT looks like.
        let style = RenderStyle::default();
        let ora = for_cursor_sql("r", &query, &style, ORA).expect("renders");
        let pg = for_cursor_sql("r", &query, &style, PG).expect("renders");
        assert!(ora.contains("IN (DELETE FROM X"), "{ora}");
        assert!(pg.contains("IN DELETE FROM X"), "{pg}");
        assert!(pg.ends_with("LOOP\n  NULL; -- TODO\nEND LOOP;"), "{pg}");
    }
}
