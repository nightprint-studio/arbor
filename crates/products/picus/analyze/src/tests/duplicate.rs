//! DUP001 / DUP002.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

// ── DUP001 ───────────────────────────────────────────────────────────────────

#[test]
fn the_same_row_inserted_twice_in_one_script_is_reported_at_the_second() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO LISTINI (COD, DESCR) VALUES ('STD2026', 'standard');\n\
         INSERT INTO PARAMETRI (COD) VALUES ('X');\n\
         INSERT INTO LISTINI (COD, DESCR) VALUES ('STD2026', 'standard');",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dup001);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].line, Some(3));
    assert_eq!(findings[0].also_at.as_deref(), Some("ORACLE/AGGIORNAMENTO/4_12__4_13.sql:1"));
    assert!(findings[0].consequence.contains("COD='STD2026'"), "{}", findings[0].consequence);
}

#[test]
fn two_rows_that_differ_anywhere_are_not_duplicates() {
    // Without a schema Picus cannot know which columns are the key, so it
    // compares the whole row. Guessing that the first column is the key is how a
    // tool tells someone their correct script is broken.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO LISTINI (COD, DESCR) VALUES ('STD2026', 'standard');\n\
         INSERT INTO LISTINI (COD, DESCR) VALUES ('STD2026', 'ridotto');",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

#[test]
fn column_order_does_not_hide_a_duplicate() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO LISTINI (COD, DESCR) VALUES ('STD2026', 'standard');\n\
         INSERT INTO LISTINI (DESCR, COD) VALUES ('standard', 'STD2026');",
    )]);
    assert_eq!(open_of(&repo.report(), RuleId::Dup001).len(), 1);
}

#[test]
fn rows_stamped_with_a_computed_value_are_not_claimed_to_be_duplicates() {
    // Two rows whose cells are both `SYSDATE` are not known to be equal.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO LOG (COD, QUANDO) VALUES ('X', SYSDATE);\n\
         INSERT INTO LOG (COD, QUANDO) VALUES ('X', SYSDATE);",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

#[test]
fn a_named_row_and_a_positional_one_are_not_lined_up() {
    // `INSERT INTO T VALUES (…)` binds to the table's physical column order,
    // which Picus does not know. Matching it against a named row would be a guess
    // about the schema dressed up as a finding. (DML002 reports it separately.)
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO LISTINI (COD) VALUES ('STD2026');\n\
         INSERT INTO LISTINI VALUES ('STD2026');",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

#[test]
fn the_same_row_in_two_different_scripts_is_not_this_rule() {
    // Two scripts inserting the same row is ordinary — an init file and the
    // update that back-fills it for existing databases.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

// ── DUP002 ───────────────────────────────────────────────────────────────────

#[test]
fn an_object_created_in_two_files_of_one_branch_is_reported() {
    let repo = Fixture::build(&[
        (
            "ORACLE/PROCEDURE/PKG_CLIENTI.sql",
            "CREATE PACKAGE BODY PKG_CLIENTI AS PROCEDURE P IS BEGIN NULL; END; END;",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "CREATE PACKAGE BODY PKG_CLIENTI AS PROCEDURE P IS BEGIN NULL; END; END;",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dup002);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].also_at.is_some());
    assert!(findings[0].consequence.contains("file order"), "{}", findings[0].consequence);
}

#[test]
fn the_same_table_created_in_both_branches_is_the_point_not_a_duplicate() {
    // If this fired, every object in every Picus repository would be a finding.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Dup002).is_empty());
}

#[test]
fn a_table_created_once_and_altered_later_is_a_maintained_table() {
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "ALTER TABLE PARAMETRI ADD (DESCR VARCHAR2(200));",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Dup002).is_empty());
}

#[test]
fn a_package_spec_and_its_body_are_not_the_same_object_defined_twice() {
    // They share a name by construction, and an inventory row, and they are
    // supposed to live in two files.
    let repo = Fixture::build(&[
        ("ORACLE/PROCEDURE/PKG_SPEC.sql", "CREATE PACKAGE PKG_CLIENTI AS PROCEDURE P; END;"),
        (
            "ORACLE/PROCEDURE/PKG_BODY.sql",
            "CREATE PACKAGE BODY PKG_CLIENTI AS PROCEDURE P IS BEGIN NULL; END; END;",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Dup002).is_empty());
}
