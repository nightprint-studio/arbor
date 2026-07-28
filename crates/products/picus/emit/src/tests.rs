//! Golden tests — the emitted SQL, asserted verbatim.
//!
//! Verbatim on purpose. A test that checked "contains INSERT" would pass while the
//! output drifted into something a reviewer would reject, and this output goes into
//! a repository a team maintains by hand: its exact shape *is* the product. When
//! one of these fails, read the diff and decide whether the new text is better —
//! that decision is the point of the test.
//!
//! The pairs matter most. The same model emitted for Oracle and for PostgreSQL must
//! be two correct spellings of one change; these tests are where that stops being
//! an aspiration.

use picus_ast::prelude::*;

use crate::emit_for_target;

fn col(name: &str, ty: &str) -> Column {
    Column {
        name: name.to_string(),
        data_type: ty.to_string(),
        primary_key: false,
        not_null: false,
        default_value: None,
    }
}

fn row(pairs: &[(&str, &str)]) -> DmlRow {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Two rows of a three-column parameters table, keyed on the code.
fn model(operation: DmlOperation) -> DmlModel {
    DmlModel {
        table: "PARAMETRI".to_string(),
        operation,
        columns: vec![
            col("COD_PARAMETRO", "varchar(20)"),
            col("VALORE", "numeric(10,2)"),
            col("DESCRIZIONE", "varchar(200)"),
        ],
        key_columns: vec![col("COD_PARAMETRO", "varchar(20)")],
        rows: vec![
            row(&[("COD_PARAMETRO", "SOGLIA_MAX"), ("VALORE", "1500"), ("DESCRIZIONE", "Soglia massima")]),
            row(&[("COD_PARAMETRO", "GIORNI_RETE"), ("VALORE", "30"), ("DESCRIZIONE", "Giorni di rete")]),
        ],
        lowercase_postgres: false,
        version_table: VersionTableConfig::default(),
    }
}

fn target(dialect: EngineKind, wrap: TargetWrap, guards: TargetGuards) -> Target {
    scoped(DialectScope::One(dialect), wrap, guards)
}

fn scoped(scope: DialectScope, wrap: TargetWrap, guards: TargetGuards) -> Target {
    Target {
        id: "t".to_string(),
        file: "x.sql".to_string(),
        dialect: scope,
        role: FolderRole::Update,
        enabled: true,
        wrap,
        guards,
    }
}

/// Emit, and fail the test on a refusal — the refusals have tests of their own.
fn emitted(model: &DmlModel, target: &Target) -> String {
    emit_for_target(model, target).expect("this target accepts this model")
}

fn plain(dialect: EngineKind) -> Target {
    target(dialect, TargetWrap::Plain, TargetGuards::default())
}

// ── Bare statements ──────────────────────────────────────────────────────────

#[test]
fn insert_oracle() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    assert_eq!(
        emitted(&m, &plain(EngineKind::Oracle)),
        "\
-- PARAMETRI · Oracle · update
INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');"
    );
}

#[test]
fn insert_postgres_lowercases_only_when_asked() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    m.lowercase_postgres = true;
    assert_eq!(
        emitted(&m, &plain(EngineKind::Postgres)),
        "\
-- PARAMETRI · PostgreSQL · update
INSERT INTO parametri (cod_parametro, valore, descrizione)
VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');"
    );
}

#[test]
fn update_assigns_everything_but_the_key() {
    let mut m = model(DmlOperation::Update);
    m.rows.truncate(1);
    assert_eq!(
        emitted(&m, &plain(EngineKind::Oracle)),
        "\
-- PARAMETRI · Oracle · update
UPDATE PARAMETRI SET VALORE = 1500, DESCRIZIONE = 'Soglia massima'
 WHERE COD_PARAMETRO = 'SOGLIA_MAX';"
    );
}

#[test]
fn delete_uses_the_key_alone() {
    let mut m = model(DmlOperation::Delete);
    m.rows.truncate(1);
    assert_eq!(
        emitted(&m, &plain(EngineKind::Oracle)),
        "\
-- PARAMETRI · Oracle · update
DELETE FROM PARAMETRI
 WHERE COD_PARAMETRO = 'SOGLIA_MAX';"
    );
}

#[test]
fn upsert_is_a_merge_on_oracle() {
    let mut m = model(DmlOperation::Upsert);
    m.rows.truncate(1);
    assert_eq!(
        emitted(&m, &plain(EngineKind::Oracle)),
        "\
-- PARAMETRI · Oracle · update
MERGE INTO PARAMETRI d
USING (SELECT 'SOGLIA_MAX' AS COD_PARAMETRO FROM DUAL) s
   ON (d.COD_PARAMETRO = s.COD_PARAMETRO)
WHEN MATCHED THEN UPDATE SET d.VALORE = 1500, d.DESCRIZIONE = 'Soglia massima'
WHEN NOT MATCHED THEN INSERT (COD_PARAMETRO, VALORE, DESCRIZIONE) VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');"
    );
}

#[test]
fn upsert_is_on_conflict_on_postgres() {
    let mut m = model(DmlOperation::Upsert);
    m.rows.truncate(1);
    assert_eq!(
        emitted(&m, &plain(EngineKind::Postgres)),
        "\
-- PARAMETRI · PostgreSQL · update
INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
VALUES ('SOGLIA_MAX', 1500, 'Soglia massima')
ON CONFLICT (COD_PARAMETRO) DO UPDATE
   SET VALORE = EXCLUDED.VALORE, DESCRIZIONE = EXCLUDED.DESCRIZIONE;"
    );
}

#[test]
fn several_rows_are_separated_by_a_blank_line() {
    let m = model(DmlOperation::Insert);
    let out = emitted(&m, &plain(EngineKind::Oracle));
    assert!(out.contains("'Soglia massima');\n\nINSERT INTO"), "got:\n{out}");
}

// ── Guarded blocks ───────────────────────────────────────────────────────────

#[test]
fn a_version_guarded_oracle_block() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let guards = TargetGuards {
        version: Some(VersionGuard { from: "4.12".into(), to: "4.13".into() }),
        ..TargetGuards::default()
    };
    assert_eq!(
        emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards)),
        "\
-- PARAMETRI · Oracle · update
DECLARE
  v_version VARCHAR2(30);
BEGIN
  -- guard: only applies when starting from 4.12
  SELECT VERSIONE INTO v_version FROM VERSIONE_DB;
  IF v_version <> '4.12' THEN
    RETURN;
  END IF;

    INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
    VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');

  -- carry the database to 4.13
  UPDATE VERSIONE_DB SET VERSIONE = '4.13', DATA_AGG = SYSDATE;
  COMMIT;
END;
/"
    );
}

#[test]
fn a_version_guarded_postgres_block() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let guards = TargetGuards {
        version: Some(VersionGuard { from: "4.12".into(), to: "4.13".into() }),
        ..TargetGuards::default()
    };
    assert_eq!(
        emitted(&m, &target(EngineKind::Postgres, TargetWrap::Block, guards)),
        "\
-- PARAMETRI · PostgreSQL · update
DO $$
DECLARE
  v_version text;
BEGIN
  -- guard: only applies when starting from 4.12
  SELECT VERSIONE INTO v_version FROM VERSIONE_DB;
  IF v_version <> '4.12' THEN
    RETURN;
  END IF;

    INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
    VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');

  -- carry the database to 4.13
  UPDATE VERSIONE_DB SET VERSIONE = '4.13', DATA_AGG = CURRENT_TIMESTAMP;
END $$;"
    );
}

#[test]
fn a_project_without_a_date_column_gets_an_update_that_omits_it() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    m.version_table = VersionTableConfig {
        table: "APP_VERSION".into(),
        version_column: "V".into(),
        date_column: None,
        filter: String::new(),
    };
    let guards = TargetGuards {
        version: Some(VersionGuard { from: "1".into(), to: "2".into() }),
        ..TargetGuards::default()
    };
    let out = emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards));

    assert!(out.contains("UPDATE APP_VERSION SET V = '2';"), "got:\n{out}");
    // Inventing a date column would emit an UPDATE that fails on the first run.
    assert!(!out.contains("SYSDATE"), "no date column means no date stamped:\n{out}");
}

#[test]
fn a_per_module_version_table_carries_its_filter_into_both_reads_and_writes() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    m.version_table = VersionTableConfig { filter: "MODULE = 'CORE'".into(), ..Default::default() };
    let guards = TargetGuards {
        version: Some(VersionGuard { from: "1".into(), to: "2".into() }),
        ..TargetGuards::default()
    };
    let out = emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards));

    assert!(out.contains("FROM VERSIONE_DB\n   WHERE MODULE = 'CORE';"), "got:\n{out}");
    assert!(
        out.contains("DATA_AGG = SYSDATE\n   WHERE MODULE = 'CORE';"),
        "the UPDATE must be filtered too, or it stamps every module:\n{out}"
    );
}

#[test]
fn skip_if_present_guards_each_row() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let guards = TargetGuards { skip_if_present: true, ..TargetGuards::default() };
    assert_eq!(
        emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards)),
        "\
-- PARAMETRI · Oracle · update
DECLARE
  v_existing NUMBER;
BEGIN
  SELECT COUNT(*) INTO v_existing FROM PARAMETRI
   WHERE COD_PARAMETRO = 'SOGLIA_MAX';
  IF v_existing = 0 THEN
    INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
    VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');
  END IF;
  COMMIT;
END;
/"
    );
}

#[test]
fn skip_if_present_does_not_wrap_a_delete() {
    let mut m = model(DmlOperation::Delete);
    m.rows.truncate(1);
    let guards = TargetGuards { skip_if_present: true, ..TargetGuards::default() };
    let out = emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards));
    // Deleting a row that isn't there is already a no-op; guarding it is noise.
    assert!(!out.contains("v_existing = 0"), "got:\n{out}");
}

#[test]
fn require_object_uses_each_engines_own_catalogue() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let guards = TargetGuards { require_object: true, ..TargetGuards::default() };

    let oracle = emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards.clone()));
    assert!(oracle.contains("FROM USER_TABLES WHERE TABLE_NAME = 'PARAMETRI'"), "got:\n{oracle}");

    let pg = emitted(&m, &target(EngineKind::Postgres, TargetWrap::Block, guards));
    assert!(pg.contains("IF to_regclass('PARAMETRI') IS NULL THEN"), "got:\n{pg}");
}

#[test]
fn a_transactional_oracle_block_rolls_back_to_its_savepoint() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let guards = TargetGuards { transactional: true, ..TargetGuards::default() };
    let out = emitted(&m, &target(EngineKind::Oracle, TargetWrap::Block, guards));

    assert!(out.contains("  SAVEPOINT before_changes;"), "got:\n{out}");
    assert!(out.contains("    ROLLBACK TO before_changes;\n    RAISE;"), "got:\n{out}");
}

#[test]
fn a_postgres_block_never_commits() {
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let out = emitted(
        &m,
        &target(EngineKind::Postgres, TargetWrap::Block, TargetGuards::default()),
    );
    // A DO block runs inside the caller's transaction and PostgreSQL refuses a
    // COMMIT there — the Oracle side commits because its block IS the transaction.
    assert!(!out.contains("COMMIT"), "got:\n{out}");
    assert!(out.ends_with("END $$;"), "got:\n{out}");
}

// ── The product's promise ────────────────────────────────────────────────────

#[test]
fn one_model_two_dialects_and_neither_borrows_the_others_syntax() {
    let m = model(DmlOperation::Upsert);
    let oracle = emitted(&m, &plain(EngineKind::Oracle));
    let pg = emitted(&m, &plain(EngineKind::Postgres));

    assert!(oracle.contains("FROM DUAL") && !oracle.contains("ON CONFLICT"));
    assert!(pg.contains("ON CONFLICT") && !pg.contains("FROM DUAL"));
    // Same source of truth: both mention every row.
    for out in [&oracle, &pg] {
        assert!(out.contains("SOGLIA_MAX") && out.contains("GIORNI_RETE"), "got:\n{out}");
    }
}

#[test]
fn emission_is_deterministic() {
    let m = model(DmlOperation::Upsert);
    let t = target(
        EngineKind::Oracle,
        TargetWrap::Block,
        TargetGuards {
            version: Some(VersionGuard { from: "4.12".into(), to: "4.13".into() }),
            skip_if_present: true,
            require_object: true,
            transactional: true,
        },
    );
    // The whole product rests on this: same input, byte-identical output, every
    // time. Not a tautology in Rust — a HashMap anywhere in the row handling would
    // break it.
    let first = emitted(&m, &t);
    for _ in 0..8 {
        assert_eq!(emitted(&m, &t), first);
    }
}

#[test]
fn an_empty_model_says_so_instead_of_emitting_nothing() {
    let mut m = model(DmlOperation::Insert);
    m.rows.clear();
    assert_eq!(
        emitted(&m, &plain(EngineKind::Oracle)),
        "-- no rows yet: fill in the form, paste some INSERTs, or import a CSV"
    );
}

#[test]
fn an_omitted_column_is_absent_from_the_statement_not_null() {
    let mut m = model(DmlOperation::Insert);
    m.rows = vec![row(&[("COD_PARAMETRO", "X"), ("VALORE", "1")])];
    let out = emitted(&m, &plain(EngineKind::Oracle));
    // The column keeps its server-side default; writing NULL would destroy it.
    assert!(!out.contains("DESCRIZIONE"), "got:\n{out}");
    assert!(out.contains("(COD_PARAMETRO, VALORE)"), "got:\n{out}");
}

// ── Portable destinations: one file that satisfies both engines ──────────────

fn portable() -> Target {
    scoped(DialectScope::Portable, TargetWrap::Plain, TargetGuards::default())
}

#[test]
fn a_portable_insert_is_one_statement_valid_on_both_engines() {
    // The payoff, and the reason the user chose this over refusing generation
    // into a portable folder: **one file instead of two**, for exactly the simple
    // inserts these folders hold.
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    let out = emitted(&m, &portable());
    assert_eq!(
        out,
        "\
-- PARAMETRI · Portable SQL · update
INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
VALUES ('SOGLIA_MAX', 1500, 'Soglia massima');"
    );
    // Byte-for-byte the Oracle statement, minus the header — which is the point:
    // the intersection is not a third dialect, it is what both already accept.
    let oracle = emitted(&m, &plain(EngineKind::Oracle));
    assert_eq!(out.lines().skip(1).collect::<Vec<_>>(), oracle.lines().skip(1).collect::<Vec<_>>());
}

#[test]
fn a_portable_target_never_folds_identifiers_even_when_the_project_asks() {
    // `lowercase_postgres` is a PostgreSQL convention; applying it to a file
    // Oracle also reads would produce identifiers Oracle folds the other way.
    let mut m = model(DmlOperation::Insert);
    m.rows.truncate(1);
    m.lowercase_postgres = true;

    assert!(emitted(&m, &portable()).contains("INSERT INTO PARAMETRI (COD_PARAMETRO"));
    // …while the PostgreSQL target still honours it.
    assert!(emitted(&m, &plain(EngineKind::Postgres)).contains("INSERT INTO parametri"));
}

#[test]
fn a_portable_target_writes_the_now_both_engines_accept() {
    let mut m = model(DmlOperation::Insert);
    m.rows = vec![row(&[("COD_PARAMETRO", "X"), ("DESCRIZIONE", "SYSDATE")])];
    let out = emitted(&m, &portable());
    assert!(out.contains("CURRENT_TIMESTAMP"), "got:\n{out}");
    assert!(!out.contains("SYSDATE"), "got:\n{out}");
}

#[test]
fn a_portable_target_refuses_an_upsert_rather_than_picking_an_engine() {
    // The invariant: nothing may be emitted into a portable folder that would run
    // on only one engine. It is a refusal from the *emitter*, so a caller cannot
    // reach the wrong output by forgetting to check first.
    let m = model(DmlOperation::Upsert);
    let refusal = emit_for_target(&m, &portable()).expect_err("must refuse");
    assert!(refusal.contains("MERGE"), "{refusal}");
    assert!(refusal.contains("ON CONFLICT"), "{refusal}");
    // …and the same refusal is what the target reports up front, word for word,
    // so the preview and the write cannot disagree about why.
    assert_eq!(portable().refuses(&m).as_deref(), Some(refusal));
}

#[test]
fn a_portable_target_refuses_a_block_and_therefore_a_version_guard() {
    let m = model(DmlOperation::Insert);
    let blocked = scoped(DialectScope::Portable, TargetWrap::Block, TargetGuards::default());
    let refusal = emit_for_target(&m, &blocked).expect_err("must refuse");
    assert!(refusal.contains("portable"), "{refusal}");

    let guards = TargetGuards {
        version: Some(VersionGuard { from: "4.12".into(), to: "4.13".into() }),
        ..TargetGuards::default()
    };
    let guarded = scoped(DialectScope::Portable, TargetWrap::Plain, guards);
    let stated = guarded.refuses(&m).expect("refused before anything is emitted");
    assert!(stated.contains("procedural block"), "{stated}");
}

#[test]
fn every_plain_operation_a_portable_folder_is_for_is_accepted() {
    // Simple INSERT / UPDATE / DELETE is exactly the case the user has, and the
    // reason generation into these folders is allowed at all.
    for operation in [DmlOperation::Insert, DmlOperation::Update, DmlOperation::Delete] {
        let mut m = model(operation);
        m.rows.truncate(1);
        let out = emit_for_target(&m, &portable()).expect("accepted");
        assert!(out.contains("PARAMETRI"), "{operation:?}: {out}");
    }
}
