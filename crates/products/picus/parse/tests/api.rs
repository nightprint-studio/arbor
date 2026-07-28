//! What the wrapper API reports: statement boundaries and kinds, objects,
//! DML shape, foreign constructs, literals and errors.

use picus_parse::prelude::*;

fn oracle(sql: &str) -> ParsedFile {
    parse(sql, DialectScope::One(EngineKind::Oracle))
}

fn postgres(sql: &str) -> ParsedFile {
    parse(sql, DialectScope::One(EngineKind::Postgres))
}

fn kinds(file: &ParsedFile) -> Vec<StatementKind> {
    file.statements.iter().map(|s| s.kind).collect()
}

// ── Statement boundaries ────────────────────────────────────────────────────

#[test]
fn statements_are_split_on_the_terminator() {
    let sql = "SELECT 1; SELECT 2; SELECT 3;";
    let parsed = postgres(sql);
    assert_eq!(kinds(&parsed), vec![StatementKind::Select; 3]);
    assert_eq!(parsed.statements[0].range.slice(sql), "SELECT 1;");
    assert_eq!(parsed.statements[2].range.slice(sql), "SELECT 3;");
}

#[test]
fn a_statement_range_includes_its_terminator() {
    // A rewriter deleting the range must not leave an orphan `;` or `/`.
    let sql = "BEGIN\n  NULL;\nEND;\n/\n";
    let parsed = oracle(sql);
    assert_eq!(kinds(&parsed), vec![StatementKind::Block]);
    assert_eq!(parsed.statements[0].range.slice(sql), "BEGIN\n  NULL;\nEND;\n/");
}

#[test]
fn a_semicolon_inside_a_string_does_not_split_a_statement() {
    let sql = "INSERT INTO t (a) VALUES ('uno; due');";
    let parsed = postgres(sql);
    assert_eq!(parsed.statements.len(), 1);
}

#[test]
fn a_semicolon_inside_a_dollar_quoted_body_does_not_split_a_statement() {
    let sql = "CREATE FUNCTION f () RETURNS void AS $$ BEGIN INSERT INTO t VALUES (1); END; $$ LANGUAGE plpgsql;";
    let parsed = postgres(sql);
    assert_eq!(parsed.statements.len(), 1);
    assert_eq!(kinds(&parsed), vec![StatementKind::Create]);
}

#[test]
fn comments_between_statements_land_in_the_gaps() {
    let sql = "-- primo\nSELECT 1;\n/* secondo */\nSELECT 2;\n";
    let parsed = postgres(sql);
    assert_eq!(parsed.statements.len(), 2);
    let gaps: Vec<&str> = parsed
        .segments()
        .iter()
        .filter_map(|s| match s {
            Segment::Gap(r) => Some(r.slice(sql)),
            Segment::Statement(_) => None,
        })
        .collect();
    assert_eq!(gaps, vec!["-- primo\n", "\n/* secondo */\n", "\n"]);
}

#[test]
fn statement_at_finds_the_statement_under_an_offset() {
    let sql = "SELECT 1; SELECT 2;";
    let parsed = postgres(sql);
    assert_eq!(parsed.statement_at(3).map(|s| s.range.start), Some(0));
    assert_eq!(parsed.statement_at(12).map(|s| s.range.start), Some(10));
    // The space between them belongs to no statement.
    assert!(parsed.statement_at(9).is_none());
}

#[test]
fn every_statement_kind_is_recognised() {
    let sql = "
SELECT 1;
INSERT INTO t (a) VALUES (1);
UPDATE t SET a = 1;
DELETE FROM t;
MERGE INTO t USING u ON (t.id = u.id) WHEN MATCHED THEN DELETE;
TRUNCATE TABLE t;
CREATE TABLE t (a integer);
ALTER TABLE t ADD b integer;
DROP TABLE t;
COMMENT ON TABLE t IS 'x';
GRANT SELECT ON t TO r;
REVOKE SELECT ON t FROM r;
SET search_path TO app;
COMMIT;
BEGIN NULL; END;
";
    assert_eq!(
        kinds(&oracle(sql)),
        vec![
            StatementKind::Select,
            StatementKind::Insert,
            StatementKind::Update,
            StatementKind::Delete,
            StatementKind::Merge,
            StatementKind::Truncate,
            StatementKind::Create,
            StatementKind::Alter,
            StatementKind::Drop,
            StatementKind::Comment,
            StatementKind::Grant,
            StatementKind::Revoke,
            StatementKind::Set,
            StatementKind::Transaction,
            StatementKind::Block,
        ]
    );
}

// ── Objects ─────────────────────────────────────────────────────────────────

#[test]
fn create_defines_and_select_references() {
    let parsed = postgres("CREATE TABLE app.t (a integer);");
    let defined = &parsed.statements[0].defines;
    assert_eq!(defined.len(), 1);
    assert_eq!(defined[0].kind, ObjectKind::Table);
    assert_eq!(defined[0].folded_qualified(), "APP.T");
    assert!(parsed.statements[0].references.is_empty());
}

#[test]
fn a_drop_references_rather_than_defines() {
    // A DROP names an object that must already exist; counting it as a
    // definition would put a removed table into the inventory.
    let parsed = postgres("DROP TABLE t;");
    assert!(parsed.statements[0].defines.is_empty());
    let refs = &parsed.statements[0].references;
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, ObjectKind::Table);
    assert_eq!(refs[0].folded_name(), "T");
}

#[test]
fn drop_carries_the_object_kind_from_its_keywords() {
    let parsed = postgres("DROP MATERIALIZED VIEW mv;");
    assert_eq!(parsed.statements[0].references[0].kind, ObjectKind::MaterializedView);
    let parsed = oracle("DROP PACKAGE BODY pkg;");
    assert_eq!(parsed.statements[0].references[0].kind, ObjectKind::PackageBody);
}

#[test]
fn a_query_references_every_table_it_names() {
    let parsed = postgres("SELECT a FROM t1 JOIN app.t2 ON t1.id = t2.id WHERE b IN (SELECT x FROM t3);");
    let names: Vec<String> =
        parsed.statements[0].references.iter().map(|r| r.folded_qualified()).collect();
    assert_eq!(names, vec!["T1", "APP.T2", "T3"]);
}

#[test]
fn a_foreign_key_references_the_parent_table() {
    let parsed = postgres(
        "CREATE TABLE figlio (p integer, CONSTRAINT fk FOREIGN KEY (p) REFERENCES padre (id));",
    );
    let statement = &parsed.statements[0];
    assert_eq!(statement.defines[0].folded_name(), "FIGLIO");
    assert_eq!(statement.references[0].folded_name(), "PADRE");
}

#[test]
fn an_index_defines_itself_and_references_its_table() {
    let parsed = postgres("CREATE INDEX ix ON t (a);");
    let statement = &parsed.statements[0];
    assert_eq!(statement.defines[0].kind, ObjectKind::Index);
    assert_eq!(statement.defines[0].folded_name(), "IX");
    assert_eq!(statement.references[0].folded_name(), "T");
}

#[test]
fn names_fold_the_same_way_across_the_two_branches() {
    // The whole cross-dialect diff rests on this: an Oracle PARAMETRI and a
    // PostgreSQL parametri have to compare equal.
    let a = oracle("SELECT 1 FROM PARAMETRI;");
    let b = postgres("select 1 from parametri;");
    assert_eq!(
        a.statements[0].references[0].folded_name(),
        b.statements[0].references[0].folded_name()
    );
    // …and a quoted name must NOT.
    let c = postgres("select 1 from \"parametri\";");
    assert_ne!(
        a.statements[0].references[0].folded_name(),
        c.statements[0].references[0].folded_name()
    );
}

#[test]
fn a_database_link_is_not_part_of_the_name() {
    let parsed = oracle("SELECT a FROM t@remoto;");
    assert_eq!(parsed.statements[0].references[0].folded_name(), "T");
}

// ── DML shape ───────────────────────────────────────────────────────────────

#[test]
fn insert_reports_its_columns_and_rows() {
    let sql = "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('A', '1'), ('B', '2');";
    let parsed = postgres(sql);
    let shape = &parsed.statements[0].dml[0];
    assert_eq!(shape.operation, DmlOperation::Insert);
    assert_eq!(shape.table.folded_name(), "PARAMETRI");
    assert!(shape.has_column_list);
    let columns: Vec<String> = shape.columns.iter().map(|c| c.folded_name()).collect();
    assert_eq!(columns, vec!["COD", "VALORE"]);
    assert_eq!(shape.rows.len(), 2);
    assert_eq!(
        shape.rows[0].values[0].literal,
        Some(LiteralValue::String("A".into()))
    );
    assert_eq!(shape.rows[1].range.slice(sql), "('B', '2')");
}

#[test]
fn a_column_less_insert_is_distinguishable_from_an_empty_column_list() {
    let with = postgres("INSERT INTO t (a) VALUES (1);");
    let without = postgres("INSERT INTO t VALUES (1);");
    assert!(with.statements[0].dml[0].has_column_list);
    assert!(!without.statements[0].dml[0].has_column_list);
    assert!(without.statements[0].dml[0].columns.is_empty());
}

#[test]
fn a_default_keyword_keeps_its_position_in_the_row() {
    // If DEFAULT were dropped, every cell after it would line up against the
    // wrong column and a duplicate-key check would compare the wrong values.
    let sql = "INSERT INTO t (a, b, c) VALUES (1, DEFAULT, 3);";
    let parsed = postgres(sql);
    let row = &parsed.statements[0].dml[0].rows[0];
    assert_eq!(row.values.len(), 3);
    assert_eq!(row.values[1].range.slice(sql), "DEFAULT");
    assert_eq!(row.values[1].literal, None);
    assert_eq!(row.values[2].literal, Some(LiteralValue::Number("3".into())));
}

#[test]
fn a_computed_value_has_a_range_but_no_literal() {
    let sql = "INSERT INTO t (a, b) VALUES ('x', SYSDATE);";
    let parsed = oracle(sql);
    let row = &parsed.statements[0].dml[0].rows[0];
    assert_eq!(row.values[0].literal, Some(LiteralValue::String("x".into())));
    assert_eq!(row.values[1].literal, None);
    assert_eq!(row.values[1].range.slice(sql), "SYSDATE");
}

#[test]
fn key_cells_line_the_row_up_against_the_column_list() {
    let parsed = postgres("INSERT INTO t (cod, val, note) VALUES ('A', 1, 'x');");
    let shape = &parsed.statements[0].dml[0];
    let cells = shape
        .key_cells(&shape.rows[0], &["COD".to_string()])
        .expect("the column list is present");
    assert_eq!(cells[0].literal, Some(LiteralValue::String("A".into())));
    // No column list means no positional guessing.
    let blind = postgres("INSERT INTO t VALUES ('A', 1);");
    let blind_shape = &blind.statements[0].dml[0];
    assert!(blind_shape.key_cells(&blind_shape.rows[0], &["COD".to_string()]).is_none());
}

#[test]
fn insert_from_a_query_has_no_rows_but_says_so() {
    let parsed = postgres("INSERT INTO t (a) SELECT x FROM u;");
    let shape = &parsed.statements[0].dml[0];
    assert!(shape.from_query);
    assert!(shape.rows.is_empty());
}

#[test]
fn update_reports_assignments_and_the_presence_of_where() {
    let sql = "UPDATE VERSIONE_DB SET VERSIONE = '4.13', DATA_AGG = SYSDATE WHERE MODULO = 'CORE';";
    let parsed = oracle(sql);
    let shape = &parsed.statements[0].dml[0];
    assert_eq!(shape.operation, DmlOperation::Update);
    let columns: Vec<String> = shape.assignments.iter().map(|a| a.column.folded_name()).collect();
    assert_eq!(columns, vec!["VERSIONE", "DATA_AGG"]);
    assert_eq!(
        shape.assignments[0].value.literal,
        Some(LiteralValue::String("4.13".into()))
    );
    assert_eq!(
        shape.where_clause.map(|r| r.slice(sql)),
        Some("WHERE MODULO = 'CORE'")
    );
}

#[test]
fn an_unguarded_update_reports_no_where() {
    let parsed = oracle("UPDATE VERSIONE_DB SET VERSIONE = '4.13';");
    assert!(parsed.statements[0].dml[0].where_clause.is_none());
}

#[test]
fn delete_and_returning_and_conflict_are_reported() {
    let sql = "DELETE FROM t WHERE a = 1 RETURNING id;";
    let parsed = postgres(sql);
    let shape = &parsed.statements[0].dml[0];
    assert_eq!(shape.operation, DmlOperation::Delete);
    assert_eq!(shape.returning.map(|r| r.slice(sql)), Some("RETURNING id"));

    let sql = "INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET b = 2;";
    let parsed = postgres(sql);
    let shape = &parsed.statements[0].dml[0];
    assert!(shape.conflict.is_some());
    assert_eq!(shape.assignments.len(), 1);
}

#[test]
fn a_merge_reports_its_target_columns_and_conflict_block() {
    let sql = "MERGE INTO PARAMETRI d USING (SELECT 'A' COD FROM DUAL) s ON (d.COD = s.COD) \
               WHEN MATCHED THEN UPDATE SET d.VALORE = '1' \
               WHEN NOT MATCHED THEN INSERT (COD, VALORE) VALUES ('A', '1');";
    let parsed = oracle(sql);
    let shape = &parsed.statements[0].dml[0];
    assert_eq!(shape.operation, DmlOperation::Merge);
    assert_eq!(shape.table.folded_name(), "PARAMETRI");
    assert!(shape.has_column_list, "the INSERT branch carries the column list");
    let columns: Vec<String> = shape.columns.iter().map(|c| c.folded_name()).collect();
    assert_eq!(columns, vec!["COD", "VALORE"]);
    assert_eq!(shape.rows.len(), 1);
    assert!(shape.conflict.is_some());
    assert_eq!(shape.assignments.len(), 1);
}

#[test]
fn dml_nested_in_a_plsql_block_is_reported() {
    // This is the case the product actually meets: an upgrade script's INSERT
    // lives inside DECLARE … BEGIN … END, so a walker that stopped at the block
    // would report an empty file.
    let sql = "\
DECLARE
  v NUMBER;
BEGIN
  SELECT COUNT(*) INTO v FROM PARAMETRI;
  IF v = 0 THEN
    INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', '15');
  END IF;
  UPDATE VERSIONE_DB SET VERSIONE = '4.13';
END;
/
";
    let parsed = oracle(sql);
    assert_eq!(parsed.statements.len(), 1);
    let statement = &parsed.statements[0];
    assert_eq!(statement.kind, StatementKind::Block);
    assert_eq!(statement.dml.len(), 2);
    assert_eq!(statement.dml[0].operation, DmlOperation::Insert);
    assert_eq!(statement.dml[0].table.folded_name(), "PARAMETRI");
    assert_eq!(statement.dml[1].operation, DmlOperation::Update);
    assert!(statement.dml[1].where_clause.is_none());
    // The SELECT inside the block still contributes its reference.
    let refs: Vec<String> = statement.references.iter().map(|r| r.folded_name()).collect();
    assert!(refs.contains(&"PARAMETRI".to_string()));
    assert!(refs.contains(&"VERSIONE_DB".to_string()));
}

// ── Cross-dialect findings ──────────────────────────────────────────────────

#[test]
fn oracle_constructs_are_flagged_in_a_postgres_file() {
    let sql = "SELECT NVL(a, 0) FROM t WHERE ROWNUM <= 10;";
    let parsed = postgres(sql);
    let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
    assert!(found.contains(&"rownum"), "{found:?}");
    assert!(found.contains(&"NVL"), "{found:?}");
    assert!(parsed.foreign().all(|f| f.belongs_to == EngineKind::Oracle));
}

#[test]
fn postgres_constructs_are_flagged_in_an_oracle_file() {
    let sql = "INSERT INTO t (a) VALUES (1) ON CONFLICT DO NOTHING;";
    let parsed = oracle(sql);
    let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
    assert_eq!(found, vec!["on_conflict_clause"]);
    assert_eq!(parsed.foreign().next().unwrap().belongs_to, EngineKind::Postgres);
}

#[test]
fn a_finding_points_at_the_construct_and_says_what_to_write_instead() {
    let sql = "SELECT e.a FROM emp e, emp m WHERE e.mgr = m.id(+);";
    let parsed = postgres(sql);
    let finding = parsed.foreign().next().expect("the (+) marker must be reported");
    assert_eq!(finding.construct, "oracle_outer_join");
    assert_eq!(finding.range.slice(sql), "m.id(+)");
    assert!(finding.message.contains("LEFT JOIN"), "{}", finding.message);
}

#[test]
fn a_construct_in_its_own_dialect_is_never_flagged() {
    assert_eq!(oracle("SELECT SYSDATE FROM DUAL;").foreign().count(), 0);
    assert_eq!(postgres("SELECT a::text FROM t LIMIT 1;").foreign().count(), 0);
    assert_eq!(
        oracle("BEGIN\n  EXECUTE IMMEDIATE 'x';\nEND;\n/").foreign().count(),
        0
    );
}

// ── Portable scripts: the rule inverts ──────────────────────────────────────

fn portable(sql: &str) -> ParsedFile {
    parse(sql, DialectScope::Portable)
}

#[test]
fn in_a_portable_script_both_dialects_constructs_are_flagged() {
    // The inversion, and the reason it is a better check than the one it
    // replaces: a file that promises to run on both engines may use only what
    // both understand, so `SYSDATE` and `now()` are *both* wrong here.
    let sql = "INSERT INTO t (a, b) VALUES (SYSDATE, now()) ON CONFLICT DO NOTHING;";
    let parsed = portable(sql);
    let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
    assert!(found.contains(&"SYSDATE"), "{found:?}");
    assert!(found.contains(&"NOW"), "{found:?}");
    assert!(found.contains(&"on_conflict_clause"), "{found:?}");
    // Both engines are represented, which never happens in a single-dialect file.
    assert!(parsed.foreign().any(|f| f.belongs_to == EngineKind::Oracle));
    assert!(parsed.foreign().any(|f| f.belongs_to == EngineKind::Postgres));
}

#[test]
fn a_portable_script_using_only_what_both_engines_accept_is_clean() {
    // The shape this feature exists for: plain inserts and updates, one file,
    // valid on Oracle and PostgreSQL alike.
    for sql in [
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);",
        "UPDATE PARAMETRI SET VALORE = 11 WHERE COD = 'SOGLIA';",
        "DELETE FROM PARAMETRI WHERE COD = 'VECCHIA';",
        // `CURRENT_TIMESTAMP` is standard and valid on both — which is exactly
        // why it is what the emitter writes for a portable target.
        "INSERT INTO LOG (QUANDO) VALUES (CURRENT_TIMESTAMP);",
    ] {
        let parsed = portable(sql);
        let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
        assert!(found.is_empty(), "{sql} — {found:?}");
    }
}

#[test]
fn the_oracle_upsert_idiom_is_a_finding_in_a_portable_script() {
    // `MERGE … FROM DUAL` is fine in an Oracle folder and a broken promise here.
    let sql = "MERGE INTO t d USING (SELECT 1 AS k FROM DUAL) s ON (d.k = s.k) \
               WHEN NOT MATCHED THEN INSERT (k) VALUES (1);";
    let found: Vec<&str> = portable(sql).foreign().map(|f| f.construct).collect();
    assert!(found.contains(&"dual_reference"), "{found:?}");
    // …and the same text in an Oracle folder says nothing.
    assert_eq!(oracle(sql).foreign().count(), 0);
}

#[test]
fn the_full_oracle_idiom_is_reported_construct_by_construct_in_postgres() {
    let sql = "SELECT q'[x]' FROM DUAL;";
    let parsed = postgres(sql);
    let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
    assert!(found.contains(&"q_string"), "{found:?}");
    assert!(found.contains(&"dual_reference"), "{found:?}");
}

#[test]
fn a_lone_slash_is_an_oracle_finding_in_a_postgres_file() {
    let parsed = postgres("BEGIN\n  NULL;\nEND;\n/\n");
    assert!(parsed.foreign().any(|f| f.construct == "slash_terminator"));
}

// ── Literals and lexing ─────────────────────────────────────────────────────

#[test]
fn a_line_comment_marker_inside_a_string_is_not_a_comment() {
    let sql = "INSERT INTO t (a, b) VALUES ('-- non un commento', 1);";
    let parsed = postgres(sql);
    let row = &parsed.statements[0].dml[0].rows[0];
    assert_eq!(
        row.values[0].literal,
        Some(LiteralValue::String("-- non un commento".into()))
    );
    assert_eq!(row.values[1].literal, Some(LiteralValue::Number("1".into())));
}

#[test]
fn a_q_quoted_string_may_hold_an_apostrophe() {
    let parsed = oracle("INSERT INTO t (a) VALUES (q'[l'ora]');");
    let row = &parsed.statements[0].dml[0].rows[0];
    assert_eq!(row.values[0].literal, Some(LiteralValue::String("l'ora".into())));
}

#[test]
fn q_is_only_a_q_string_when_a_delimiter_follows_the_quote() {
    // `q` on its own is an ordinary name; PostgreSQL's `q 'abc'` must not be
    // swallowed as Oracle alternative quoting.
    let parsed = postgres("SELECT q.a FROM t q WHERE q.b = 'x';");
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.foreign().count(), 0);
}

#[test]
fn a_nested_block_comment_closes_at_the_right_place() {
    // A non-nesting lexer would end the comment at the first `*/` and try to
    // parse `ancora fuori */` as SQL.
    let parsed = postgres("/* fuori /* dentro */ ancora fuori */ SELECT 1;");
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.statements.len(), 1);
    assert_eq!(parsed.statements[0].kind, StatementKind::Select);
}

#[test]
fn a_quote_inside_a_block_comment_is_not_a_string() {
    let parsed = postgres("/* l'apostrofo */ SELECT 1;");
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.statements.len(), 1);
}

#[test]
fn division_is_not_a_statement_terminator() {
    let sql = "SELECT a / b FROM t;";
    let parsed = postgres(sql);
    assert_eq!(parsed.statements.len(), 1);
    assert_eq!(parsed.statements[0].range.slice(sql), sql);
    assert_eq!(parsed.foreign().count(), 0);
}

#[test]
fn a_slash_starting_a_continuation_line_is_still_division() {
    let sql = "SELECT a\n/ b FROM t;";
    let parsed = postgres(sql);
    assert_eq!(parsed.statements.len(), 1);
    assert_eq!(parsed.errors.len(), 0);
}

#[test]
fn keywords_are_case_insensitive() {
    for sql in ["SELECT a FROM t;", "select a from t;", "SeLeCt a FrOm t;"] {
        let parsed = postgres(sql);
        assert_eq!(kinds(&parsed), vec![StatementKind::Select], "{sql}");
        assert_eq!(parsed.statements[0].references[0].folded_name(), "T", "{sql}");
    }
}

#[test]
fn a_column_whose_name_starts_with_a_keyword_is_one_identifier() {
    // `DATA_MOD` must not lex as the keyword DATA followed by `_MOD`.
    let parsed = oracle("INSERT INTO t (DATA_MOD, TYPE_ID) VALUES (SYSDATE, 1);");
    assert_eq!(parsed.errors.len(), 0);
    let columns: Vec<String> =
        parsed.statements[0].dml[0].columns.iter().map(|c| c.folded_name()).collect();
    assert_eq!(columns, vec!["DATA_MOD", "TYPE_ID"]);
}

#[test]
fn the_common_column_definition_parses() {
    // `DEFAULT 0 NOT NULL` is the single most common column definition, and the
    // NOT is one token away from being read as the start of NOT IN / NOT LIKE.
    for sql in [
        "CREATE TABLE t (c NUMBER DEFAULT 0 NOT NULL);",
        "ALTER TABLE t ADD COLUMN c NUMBER DEFAULT 0 NOT NULL;",
        "CREATE TABLE t (c VARCHAR2(10) DEFAULT 'x' NOT NULL);",
    ] {
        assert_eq!(oracle(sql).errors.len(), 0, "{sql}");
    }
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[test]
fn a_syntax_error_is_data_and_the_rest_of_the_file_survives() {
    let sql = "SELECT 1;\nSELECT FROM FROM;\nSELECT 2;";
    let parsed = postgres(sql);
    assert!(parsed.errors.len() > 0);
    // The good statements are still there.
    assert!(parsed.statements.iter().any(|s| s.range.slice(sql) == "SELECT 1;"));
    assert!(parsed.statements.iter().any(|s| s.range.slice(sql) == "SELECT 2;"));
    // And the round trip still holds over a broken file.
    assert_eq!(parsed.reassemble(sql), sql);
}

#[test]
fn a_truncated_file_does_not_hang_or_panic() {
    for sql in [
        "SELECT * FROM",
        "CREATE TABLE t (",
        "BEGIN",
        "/* unterminated",
        "'unterminated",
        "$$ unterminated",
        "q'[unterminated",
        "((((((((((",
        "SELECT",
    ] {
        let parsed = postgres(sql);
        assert_eq!(parsed.reassemble(sql), sql, "{sql}");
    }
}

#[test]
fn a_statement_without_a_terminator_is_still_a_statement() {
    // Picus parses live editor buffers, so the `;` the user has not typed yet
    // must not turn the whole query into an error node.
    let parsed = postgres("SELECT 1");
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(kinds(&parsed), vec![StatementKind::Select]);
    assert_eq!(parsed.statements[0].range.slice("SELECT 1"), "SELECT 1");
}

#[test]
fn only_the_last_statement_may_omit_its_terminator() {
    let sql = "SELECT 1;\nSELECT 2";
    let parsed = postgres(sql);
    assert_eq!(kinds(&parsed), vec![StatementKind::Select, StatementKind::Select]);
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.reassemble(sql), sql);
}

#[test]
fn an_empty_file_is_not_an_error() {
    let parsed = postgres("");
    assert!(parsed.statements.is_empty());
    assert!(parsed.errors.is_empty());
    assert_eq!(parsed.reassemble(""), "");
}

#[test]
fn a_comment_only_file_yields_one_gap_and_no_statements() {
    let sql = "-- soltanto un commento\n";
    let parsed = postgres(sql);
    assert!(parsed.statements.is_empty());
    assert_eq!(parsed.reassemble(sql), sql);
}

// ── Realistic scripts ───────────────────────────────────────────────────────

/// Modelled on `src/lib/ipc/picus/mock.ts` (`MOCK_FILE_TEXT`), which is a
/// real-shaped Oracle upgrade block.
const ORACLE_UPGRADE: &str = "\
-- Aggiornamento 4.12 -> 4.13
-- Parametri di sconto introdotti con la revisione listini.

DECLARE
  v_versione VARCHAR2(10);
  v_presenti NUMBER;
BEGIN
  SELECT VERSIONE INTO v_versione FROM VERSIONE_DB;
  IF v_versione <> '4.12' THEN
    RETURN;
  END IF;

  SELECT COUNT(*) INTO v_presenti FROM PARAMETRI
   WHERE COD_PARAMETRO = 'SOGLIA_SCONTO';
  IF v_presenti = 0 THEN
    INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE, DATA_MOD)
    VALUES ('SOGLIA_SCONTO', '15', 'Soglia sconto massimo applicabile', SYSDATE);
  END IF;

  UPDATE VERSIONE_DB SET VERSIONE = '4.13', DATA_AGG = SYSDATE;
  COMMIT;
END;
/
";

const POSTGRES_UPGRADE: &str = "\
-- Aggiornamento 4.12 -> 4.13
-- Stessa modifica del ramo Oracle, in sintassi PostgreSQL.

DO $$
DECLARE
  v_versione varchar(10);
BEGIN
  SELECT versione INTO v_versione FROM versione_db;
  IF v_versione <> '4.12' THEN
    RETURN;
  END IF;

  INSERT INTO parametri (cod_parametro, valore, descrizione, data_mod)
  VALUES ('SOGLIA_SCONTO', '15', 'Soglia sconto massimo applicabile', now())
  ON CONFLICT (cod_parametro) DO NOTHING;

  UPDATE versione_db SET versione = '4.13', data_agg = now();
END
$$;
";

#[test]
fn the_oracle_upgrade_script_parses_and_yields_its_change() {
    let parsed = oracle(ORACLE_UPGRADE);
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.reassemble(ORACLE_UPGRADE), ORACLE_UPGRADE);
    assert_eq!(kinds(&parsed), vec![StatementKind::Block]);

    let statement = &parsed.statements[0];
    assert_eq!(statement.dml.len(), 2);

    let insert = &statement.dml[0];
    assert_eq!(insert.table.folded_name(), "PARAMETRI");
    let columns: Vec<String> = insert.columns.iter().map(|c| c.folded_name()).collect();
    assert_eq!(columns, vec!["COD_PARAMETRO", "VALORE", "DESCRIZIONE", "DATA_MOD"]);
    assert_eq!(
        insert.rows[0].values[0].literal,
        Some(LiteralValue::String("SOGLIA_SCONTO".into()))
    );
    // The date is computed, not literal — a duplicate-key check must not treat
    // it as a value it can compare.
    assert_eq!(insert.rows[0].values[3].literal, None);

    let update = &statement.dml[1];
    assert_eq!(update.table.folded_name(), "VERSIONE_DB");
    assert!(update.where_clause.is_none());

    // Nothing in the file is foreign to Oracle.
    assert_eq!(parsed.foreign().count(), 0);
}

#[test]
fn the_postgres_upgrade_script_parses_and_yields_the_same_change() {
    let parsed = postgres(POSTGRES_UPGRADE);
    assert_eq!(parsed.errors.len(), 0);
    assert_eq!(parsed.reassemble(POSTGRES_UPGRADE), POSTGRES_UPGRADE);
    assert_eq!(kinds(&parsed), vec![StatementKind::Block]);
    assert_eq!(parsed.foreign().count(), 0);
    // The body is a dollar-quoted string, so its DML is not visible from the
    // outside — a caller that wants it re-parses the inner range. This test
    // pins that documented limit so it cannot change silently.
    assert!(parsed.statements[0].dml.is_empty());
}

#[test]
fn the_two_branches_of_the_same_change_agree_on_table_and_key() {
    let oracle_parsed = oracle(ORACLE_UPGRADE);
    let oracle_insert = &oracle_parsed.statements[0].dml[0];

    // Re-parse the PostgreSQL body: the DO block's payload is one token, so the
    // caller peels it. This is the flow `picus-analyze` will use.
    let body_start = POSTGRES_UPGRADE.find("$$").expect("the block opens with $$") + 2;
    let body_end = POSTGRES_UPGRADE.rfind("$$").expect("and closes with $$");
    let body = &POSTGRES_UPGRADE[body_start..body_end];
    let postgres_parsed = postgres(body);
    let postgres_insert = postgres_parsed
        .statements
        .iter()
        .flat_map(|s| s.dml.iter())
        .find(|d| d.operation == DmlOperation::Insert)
        .expect("the PostgreSQL branch inserts too");

    assert_eq!(
        oracle_insert.table.folded_name(),
        postgres_insert.table.folded_name()
    );
    let oracle_columns: Vec<String> =
        oracle_insert.columns.iter().map(|c| c.folded_name()).collect();
    let postgres_columns: Vec<String> =
        postgres_insert.columns.iter().map(|c| c.folded_name()).collect();
    assert_eq!(oracle_columns, postgres_columns);
    assert_eq!(
        oracle_insert.rows[0].values[0].literal,
        postgres_insert.rows[0].values[0].literal
    );
}

#[test]
fn the_oracle_script_read_as_postgres_names_every_divergence() {
    let parsed = postgres(ORACLE_UPGRADE);
    let found: Vec<&str> = parsed.foreign().map(|f| f.construct).collect();
    assert!(found.contains(&"SYSDATE"), "{found:?}");
    assert!(found.contains(&"slash_terminator"), "{found:?}");
    // And it is a report, not a parse failure: the statements are still there.
    assert_eq!(kinds(&parsed), vec![StatementKind::Block]);
}
