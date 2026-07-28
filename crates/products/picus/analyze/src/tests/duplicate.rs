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

#[test]
fn a_delete_between_two_inserts_makes_the_second_one_a_reload() {
    // The ordinary way to make an update script re-runnable, and the rule used to
    // fire on every one of them: clear the row, write it again. The second INSERT
    // is not a mistake, it is the point.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\n\
         DELETE FROM PARAMETRI WHERE COD = 'SOGLIA';\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

#[test]
fn a_delete_on_another_table_does_not_excuse_a_duplicate() {
    // The clearing is scoped to the table it names. Without that, one unrelated
    // DELETE anywhere in a file would switch the rule off for the whole file.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\n\
         DELETE FROM LISTINI WHERE COD = 'STD';\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);",
    )]);
    assert_eq!(open_of(&repo.report(), RuleId::Dup001).len(), 1);
}

#[test]
fn a_truncate_clears_the_table_the_same_way_a_delete_does() {
    // TRUNCATE is a statement rather than DML, so it reaches the rule by a
    // different path — and it is the most emphatic "forget what was in there"
    // there is.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\n\
         TRUNCATE TABLE PARAMETRI;\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);",
    )]);
    assert!(open_of(&repo.report(), RuleId::Dup001).is_empty());
}

#[test]
fn a_delete_after_both_inserts_does_not_excuse_them() {
    // Order matters: the rule walks the file, and a DELETE at the bottom cannot
    // retroactively separate two INSERTs above it.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);\n\
         DELETE FROM PARAMETRI WHERE COD = 'SOGLIA';",
    )]);
    assert_eq!(open_of(&repo.report(), RuleId::Dup001).len(), 1);
}

// ── DUP002 ───────────────────────────────────────────────────────────────────

/// Two files a *fresh install* runs, both creating the same object. This is the
/// case that is wrong under every reading of a repository: whichever runs last
/// decides what is in the database.
fn created_twice_on_install() -> Fixture {
    Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30), VALORE NUMBER);",
        ),
    ])
}

#[test]
fn an_object_created_in_two_files_of_one_dialect_is_reported() {
    let report = created_twice_on_install().report();
    let findings = open_of(&report, RuleId::Dup002);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].also_at.is_some());
    assert!(findings[0].consequence.contains("file order"), "{}", findings[0].consequence);
}

#[test]
fn a_definition_in_each_half_is_the_cumulative_model_working() {
    // The initialisation carries the object's current shape; the update that
    // introduced it carries the same CREATE, because that is how a database which
    // already exists gets it. Reporting that pair lists every object the
    // repository has ever added — the same false positive `CONS002` had, for the
    // same reason, and on the same repositories.
    let repo = || {
        Fixture::build(&[
            (
                "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
            ),
            (
                "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
                "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
            ),
        ])
    };
    assert!(open_of(&repo().report(), RuleId::Dup002).is_empty());

    // …and a project that says the two halves must mirror each other still gets
    // the finding, so this is a reading of the repository and not a hole.
    assert_eq!(open_of(&repo().mirrored().report(), RuleId::Dup002).len(), 1);

    // The within-one-half comparison is untouched by the model.
    assert_eq!(open_of(&created_twice_on_install().report(), RuleId::Dup002).len(), 1);
}

#[test]
fn the_same_table_created_in_both_dialects_is_the_point_not_a_duplicate() {
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

#[test]
fn a_wrapper_function_every_update_replaces_is_not_a_duplicate() {
    // The house style: each update script defines a throwaway wrapper holding
    // "if the version is X, do this, then set it to Y", calls it, and moves on.
    // Two hundred update scripts declare the same function two hundred times, and
    // `CREATE OR REPLACE` is the author saying that is fine.
    let repo = Fixture::build(&[
        (
            "POSTGRES/AGGIORNAMENTO/4_11__4_12.sql",
            "CREATE OR REPLACE FUNCTION aggiornamento() RETURNS void AS $$ BEGIN NULL; END; $$ LANGUAGE plpgsql;",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "CREATE OR REPLACE FUNCTION aggiornamento() RETURNS void AS $$ BEGIN NULL; END; $$ LANGUAGE plpgsql;",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Dup002).is_empty());
}

#[test]
fn a_plain_create_in_two_places_is_still_a_duplicate() {
    // Without OR REPLACE the two files genuinely race, and whichever runs last
    // decides what is in the database. The exemption above is about the stated
    // intent, not about the object being a function.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE CATALOGO_WIDGET (CHIAVE VARCHAR2(30));",
        ),
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "CREATE TABLE CATALOGO_WIDGET (CHIAVE VARCHAR2(30), ORDINE NUMBER);",
        ),
    ]);
    assert_eq!(open_of(&repo.report(), RuleId::Dup002).len(), 1);
}
